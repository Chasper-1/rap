use realfft::RealFftPlanner;
use rodio::Source;
use std::num::NonZero;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{Receiver, Sender};

pub struct VisualizableSource<S>
where
    S: Source + Send,
    S::Item: Into<f32> + Send,
{
    pub input: S,
    pub sender: Sender<f32>,
}

impl<S> Source for VisualizableSource<S>
where
    S: Source + Send,
    S::Item: Into<f32> + Send,
{
    fn channels(&self) -> NonZero<u16> {
        self.input.channels()
    }
    fn sample_rate(&self) -> NonZero<u32> {
        self.input.sample_rate()
    }
    fn current_span_len(&self) -> Option<usize> {
        self.input.current_span_len()
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.input.total_duration()
    }

    // ВОТ ЭТОГО НЕ ХВАТАЛО: Прокидываем перемотку внутрь источника
    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), rodio::source::SeekError> {
        self.input.try_seek(pos)
    }
}

impl<S> Iterator for VisualizableSource<S>
where
    S: Source + Send,
    S::Item: Into<f32> + Send,
{
    type Item = S::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.input.next()?;
        let _ = self.sender.try_send(sample.into());
        Some(sample)
    }
}

pub fn spawn_analyzer(mut rx: Receiver<f32>, output: Arc<Mutex<Vec<f32>>>) {
    tokio::spawn(async move {
        let fft_size = 2048;
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_size);
        let mut input_buffer = Vec::with_capacity(fft_size);
        let mut prev_freqs = vec![0.0; 128];
        let sample_rate = 48000.0;

        loop {
            let conf = crate::config::config::Config::global();
            let ui = &conf.ui;

            match tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await {
                Ok(Some(sample)) => {
                    input_buffer.push(sample);

                    if input_buffer.len() >= fft_size {
                        let mut out_spectrum = fft.make_output_vec();
                        let mut indata = input_buffer.clone();

                        if fft.process(&mut indata, &mut out_spectrum).is_ok() {
                            let mut current_freqs = vec![0.0; 128];
                                let get_idx = |hz: f32| ((hz * fft_size as f32) / sample_rate) as usize;
                            
                                // Лимиты столбиков (можно подвигать, если хочешь больше/меньше места под бас)
                                let b_limit = 45; // Даем басу почти 40% всего графика (было 42)
                                let m_limit = 90; // Середина до 90-го столбика
                            
                                for i in 0..128 {
                                    let (start_bin, end_bin, zone_sens) = if i < b_limit {
                                        // --- ЗОНА БАССА: 30Гц - 600Гц ---
                                        let pct_start = i as f32 / b_limit as f32;
                                        let pct_end = (i + 1) as f32 / b_limit as f32;
                                        
                                        // Квадратичное распределение, чтобы в самом низу было больше деталей
                                        let s = get_idx(30.0 + (600.0 - 30.0) * pct_start.powi(2));
                                        let e = get_idx(30.0 + (600.0 - 30.0) * pct_end.powi(2));
                                        (s, e.max(s + 1), ui.cava_sensitivity_low)
                                        
                                    } else if i < m_limit {
                                        // --- СЕРЕДИНА: 600Гц - 4500Гц ---
                                        let pct_start = (i - b_limit) as f32 / (m_limit - b_limit) as f32;
                                        let pct_end = (i - b_limit + 1) as f32 / (m_limit - b_limit) as f32;
                                        
                                        let s = get_idx(600.0 + (4500.0 - 600.0) * pct_start);
                                        let e = get_idx(600.0 + (4500.0 - 600.0) * pct_end);
                                        (s, e.max(s + 1), ui.cava_sensitivity_mid)
                                        
                                    } else {
                                        // --- ВЕРХА: 4500Гц - 16000Гц ---
                                        let pct_start = (i - m_limit) as f32 / (128 - m_limit) as f32;
                                        let pct_end = (i - m_limit + 1) as f32 / (128 - m_limit) as f32;
                                        
                                        let s = get_idx(4500.0 + (16000.0 - 4500.0) * pct_start);
                                        let e = get_idx(4500.0 + (16000.0 - 4500.0) * pct_end);
                                        (s, e.max(s + 1), ui.cava_sensitivity_high)
                                    };
                            
                                    // Считаем энергию ВНУТРИ диапазона (усреднение)
                                    let mut amp = 0.0;
                                    let chunk = &out_spectrum[start_bin..end_bin.min(out_spectrum.len())];
                                    
                                    if !chunk.is_empty() {
                                        for bin in chunk {
                                            amp += bin.norm();
                                        }
                                        amp /= chunk.len() as f32;
                                    }
                            
                                    // Применяем конфиг
                                    let adjusted_amp = if amp < ui.cava_noise_gate { 0.0 } else { amp };
                                    let pos = i as f32 / 128.0;
                                    let tilt_boost = 1.0 + (ui.cava_tilt * pos);
                            
                                    let mut val = adjusted_amp * zone_sens * tilt_boost;
                            
                                    val = val.tanh();
                                    val = val.powf(ui.cava_exponent);
                                    val = val.max(prev_freqs[i] * ui.cava_fall_speed);
                                    
                                    current_freqs[i] = val.clamp(0.0, 1.0);
                                }

                            prev_freqs = current_freqs.clone();
                            if let Ok(mut out) = output.try_lock() {
                                *out = current_freqs;
                            }
                        }
                        input_buffer.clear();
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    // Пауза: плавно гасим столбики
                    let fall = crate::config::config::Config::global().ui.cava_fall_speed;
                    for i in 0..128 {
                        prev_freqs[i] *= fall;
                    }
                    if let Ok(mut out) = output.try_lock() {
                        *out = prev_freqs.clone();
                    }
                }
            }
        }
    });
}
