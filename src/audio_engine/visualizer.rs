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
        let fft_size = 2048; // Для еще большей четкости баса можно поставить 4096
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_size);
        let mut input_buffer = Vec::with_capacity(fft_size);
        let mut prev_freqs = Vec::new();
        let sample_rate = 48000.0;

        loop {
            let conf = crate::config::config::Config::global();
            let ui = &conf.ui;

            // Узнаем, сколько столбиков от нас хочет виджет
            let target_width = if let Ok(out) = output.try_lock() {
                out.len()
            } else {
                0 // Если занято, просто пропустим этот шаг
            };

            if target_width == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                continue;
            }

            // Подгоняем буфер инерции под текущую ширину
            if prev_freqs.len() != target_width {
                prev_freqs.resize(target_width, 0.0);
            }

            match tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await {
                Ok(Some(sample)) => {
                    input_buffer.push(sample);

                    if input_buffer.len() >= fft_size {
                        let mut out_spectrum = fft.make_output_vec();
                        let mut indata = input_buffer.clone();

                        if fft.process(&mut indata, &mut out_spectrum).is_ok() {
                            let mut current_freqs = vec![0.0; target_width];
                            let get_idx = |hz: f32| ((hz * fft_size as f32) / sample_rate) as usize;

                            for i in 0..target_width {
                                // 1. Сетка частот (как в CAVA)
                                let f_min: f32 = 20.0;
                                let f_max: f32 = 12000.0; // 12кГц достаточно для терминала
                                let pct_s = i as f32 / target_width as f32;
                                let pct_e = (i + 1) as f32 / target_width as f32;

                                let ratio: f32 = f_max / f_min;
                                let start_hz = f_min * ratio.powf(pct_s);
                                let end_hz = f_min * ratio.powf(pct_e);

                                let s_idx = get_idx(start_hz);
                                let e_idx = get_idx(end_hz).max(s_idx + 1);

                                // 2. Энергия (Интеграл)
                                let mut energy = 0.0;
                                let chunk = &out_spectrum[s_idx..e_idx.min(out_spectrum.len())];

                                if !chunk.is_empty() {
                                    for bin in chunk {
                                        energy += bin.norm();
                                    }
                                    energy /= chunk.len() as f32; // Усредняем, чтобы не зависеть от ширины зоны

                                    // Tilt — единственный мягкий множитель
                                    energy *= 1.0 + (ui.cava_tilt * pct_s);
                                }

                                // 3. Noise Gate (вот он, чтобы варнинг ушел)
                                if energy < ui.cava_noise_gate {
                                    energy = 0.0;
                                }

                                // 4. Чувствительность зон
                                let zone_sens = if i < target_width / 4 {
                                    ui.cava_sensitivity_low
                                } else if i < target_width / 2 {
                                    ui.cava_sensitivity_mid
                                } else {
                                    ui.cava_sensitivity_high
                                };

                                // Считаем финальное значение
                                let val = (energy * zone_sens).powf(ui.cava_exponent);

                                // 5. Физика (Гравитация)
                                let prev = prev_freqs[i];
                                if val > prev {
                                    current_freqs[i] = prev + (val - prev) * ui.cava_attack;
                                } else {
                                    current_freqs[i] = prev * ui.cava_fall_speed;
                                }
                            }

                            // Пост-сглаживание (Плавность)
                            let mut final_freqs = current_freqs.clone();
                            for i in 1..target_width - 1 {
                                final_freqs[i] = (current_freqs[i - 1] * 0.25)
                                    + (current_freqs[i] * 0.5)
                                    + (current_freqs[i + 1] * 0.25);
                            }
                            current_freqs = final_freqs;

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
                    let fall = crate::config::config::Config::global().ui.cava_fall_speed;
                    for i in 0..target_width {
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
