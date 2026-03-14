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
        let val: f32 = sample.into();

        // ЖЕЛЕЗНАЯ ЗАСЛОНКА:
        // Если сигнал тише порога, мы ПУСКАЕМ его в динамики (чтобы звук был чистым),
        // но НЕ ШЛЕМ в анализатор. Анализатор будет спать на recv().
        if val.abs() > 0.005 {
            let _ = self.sender.try_send(val);
        }

        Some(sample)
    }
}

pub fn spawn_analyzer(mut rx: Receiver<f32>, output: Arc<Mutex<Vec<f32>>>) {
    tokio::spawn(async move {
        let fft_size = 2048;
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_size);

        let mut input_buffer = Vec::with_capacity(fft_size);
        let mut scratch_buffer = vec![0.0f32; fft_size];
        let mut prev_freqs = Vec::new();
        let sample_rate = 48000.0;
        let mut cached_indices: Vec<(usize, usize, f32)> = Vec::new();
        let mut last_width = 0;

        // Флаг, чтобы не спамить нулями в output постоянно во время паузы
        let mut is_sleeping = false;

        loop {
            // Если работы нет (буфер пуст), встаем намертво и ждем звук
            if input_buffer.is_empty() {
                while let Some(sample) = rx.recv().await {
                    if sample.abs() > 0.0001 {
                        input_buffer.push(sample);
                        is_sleeping = false; // Проснулись
                        break;
                    }
                }
            }

            let conf = crate::config::config::Config::global();
            let ui = &conf.ui;
            let target_width = if let Ok(out) = output.try_lock() {
                out.len()
            } else {
                0
            };

            if target_width == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                continue;
            }

            // Пересчет индексов (твой код без изменений)
            if target_width != last_width {
                cached_indices.clear();
                let f_min = 20.0f32;
                let f_max = 15000.0f32;
                let ratio = f_max / f_min;
                let get_idx = |hz: f32| ((hz * fft_size as f32) / sample_rate) as usize;
                for i in 0..target_width {
                    let pct_s = i as f32 / target_width as f32;
                    let pct_e = (i + 1) as f32 / target_width as f32;
                    let s_idx = get_idx(f_min * ratio.powf(pct_s));
                    let e_idx = get_idx(f_min * ratio.powf(pct_e)).max(s_idx + 1);
                    cached_indices.push((s_idx, e_idx, pct_s));
                }
                prev_freqs.resize(target_width, 0.0);
                last_width = target_width;
            }

            // Ждем данные. Таймаут 100мс — этого за глаза хватит, чтобы понять, что музыка кончилась
            match tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await {
                Ok(Some(sample)) => {
                    input_buffer.push(sample);
                    if input_buffer.len() >= fft_size {
                        let mut out_spectrum = fft.make_output_vec();
                        scratch_buffer.copy_from_slice(&input_buffer[..fft_size]);

                        if fft.process(&mut scratch_buffer, &mut out_spectrum).is_ok() {
                            let mut current_freqs = vec![0.0; target_width];
                            for (i, &(s_idx, e_idx, pct_s)) in cached_indices.iter().enumerate() {
                                let mut energy = 0.0;
                                let chunk_end = e_idx.min(out_spectrum.len());
                                if s_idx < chunk_end {
                                    let chunk = &out_spectrum[s_idx..chunk_end];
                                    for bin in chunk {
                                        energy += bin.norm();
                                    }
                                    energy /= chunk.len() as f32;
                                    energy *= 1.0 + (ui.cava_tilt * pct_s);
                                }
                                if energy < ui.cava_noise_gate {
                                    energy = 0.0;
                                }
                                let zone_sens = if i < target_width / 3 {
                                    ui.cava_sensitivity_low
                                } else if i < (target_width * 2) / 3 {
                                    ui.cava_sensitivity_mid
                                } else {
                                    ui.cava_sensitivity_high
                                };

                                current_freqs[i] =
                                    (energy * zone_sens).powf(ui.cava_exponent).min(1.0);
                            }

                            if let Ok(mut out) = output.try_lock() {
                                *out = current_freqs;
                            }
                        }
                        input_buffer.clear();
                    }
                }
                _ => {
                    // Если таймаут случился и мы еще не «спим»
                    if !is_sleeping {
                        if let Ok(mut out) = output.try_lock() {
                            let len = out.len();
                            // Шлем нули ровно один раз, чтобы виджет запустил анимацию падения
                            *out = vec![0.0; len];
                        }
                        is_sleeping = true;
                        input_buffer.clear(); // Очистка заставляет loop уйти в rx.recv().await
                    }
                }
            }
        }
    });
}
