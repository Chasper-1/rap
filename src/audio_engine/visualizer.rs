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
            // Подтягиваем ВЕСЬ конфиг
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

                            let bass_range = (get_idx(40.0), get_idx(250.0));
                            let mid_range = (get_idx(400.0), get_idx(4000.0));
                            let high_range = (get_idx(8000.0), get_idx(15000.0));

                            let bars_per_zone = 128 / 3;

                            for i in 0..128 {
                                let (start_bin, end_bin, zone_boost) = if i < bars_per_zone {
                                    let pct = i as f32 / bars_per_zone as f32;
                                    let s = bass_range.0 + (pct * (bass_range.1 - bass_range.0) as f32) as usize;
                                    (s, s + 1, 1.0) 
                                } else if i < bars_per_zone * 2 {
                                    let pct = (i - bars_per_zone) as f32 / bars_per_zone as f32;
                                    let s = mid_range.0 + (pct * (mid_range.1 - mid_range.0) as f32) as usize;
                                    (s, s + 2, 1.0)
                                } else {
                                    let pct = (i - bars_per_zone * 2) as f32 / bars_per_zone as f32;
                                    let s = high_range.0 + (pct * (high_range.1 - high_range.0) as f32) as usize;
                                    (s, s + 5, 1.0)
                                };

                                let mut amp = 0.0;
                                let chunk = &out_spectrum[start_bin..end_bin.max(start_bin + 1).min(out_spectrum.len())];
                                for bin in chunk { amp += bin.norm(); }
                                amp /= chunk.len() as f32;

                                // --- ПРИМЕНЯЕМ КОНФИГ (ОСТАВЬ ТОЛЬКО ЭТО) ---
                                
                                // 1. Noise Gate
                                let adjusted_amp = if amp < ui.cava_noise_gate { 0.0 } else { amp };
                                
                                // 2. Tilt
                                let pos = i as f32 / 128.0;
                                let tilt_boost = 1.0 + (ui.cava_tilt * pos);
                                
                                // 3. Расчет: только амплитуда, зональный буст и твоя чувствительность из конфига
                                // Теперь ты сам решаешь, насколько это должно быть громко через cava_sensitivity
                                let mut val = adjusted_amp * zone_boost * tilt_boost * ui.cava_sensitivity; 
                                
                                // 4. Мягкое ограничение (tanh)
                                // Оставляем как предохранитель, чтобы выше 1.0 не улетало физически
                                val = val.tanh(); 
                                
                                // 5. Exponent (Острота)
                                val = val.powf(ui.cava_exponent);
                                
                                // 6. Инерция (Fall Speed)
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
                    // Используем fall_speed из конфига при паузе
                    let fall = crate::config::config::Config::global().ui.cava_fall_speed;
                    for i in 0..128 { prev_freqs[i] *= fall; }
                    if let Ok(mut out) = output.try_lock() { *out = prev_freqs.clone(); }
                }
            }
        }
    });
}