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

                            // Определяем границы частот
                            let bass_range = (get_idx(40.0), get_idx(250.0));
                            let mid_range = (get_idx(400.0), get_idx(4000.0));
                            let high_range = (get_idx(8000.0), get_idx(15000.0));

                            // Делим 128 столбиков на 3 зоны
                            let b_limit = 128 / 3;           // Конец баса
                            let m_limit = (128 / 3) * 2;     // Конец середины

                            for i in 0..128 {
                                let (start_bin, end_bin, zone_sens) = if i < b_limit {
                                    // БАСС
                                    let pct = i as f32 / b_limit as f32;
                                    let s = bass_range.0 + (pct * (bass_range.1 - bass_range.0) as f32) as usize;
                                    (s, s + 1, ui.cava_sensitivity_low)
                                } else if i < m_limit {
                                    // СЕРЕДИНА
                                    let pct = (i - b_limit) as f32 / (m_limit - b_limit) as f32;
                                    let s = mid_range.0 + (pct * (mid_range.1 - mid_range.0) as f32) as usize;
                                    (s, s + 2, ui.cava_sensitivity_mid)
                                } else {
                                    // ВЕРХА
                                    let pct = (i - m_limit) as f32 / (128 - m_limit) as f32;
                                    let s = high_range.0 + (pct * (high_range.1 - high_range.0) as f32) as usize;
                                    (s, s + 5, ui.cava_sensitivity_high)
                                };

                                let mut amp = 0.0;
                                let chunk_start = start_bin;
                                let chunk_end = end_bin.max(start_bin + 1).min(out_spectrum.len());
                                let chunk = &out_spectrum[chunk_start..chunk_end];
                                
                                for bin in chunk {
                                    amp += bin.norm();
                                }
                                amp /= chunk.len() as f32;

                                // Применяем настройки из конфига
                                let adjusted_amp = if amp < ui.cava_noise_gate { 0.0 } else { amp };
                                let pos = i as f32 / 128.0;
                                let tilt_boost = 1.0 + (ui.cava_tilt * pos);

                                let mut val = adjusted_amp * zone_sens * tilt_boost;

                                // Лимитер и экспонента
                                val = val.tanh();
                                val = val.powf(ui.cava_exponent);

                                // Плавное падение
                                val = val.max(prev_freqs[i] * ui.cava_fall_speed);
                                current_freqs[i] = val.min(1.0);
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