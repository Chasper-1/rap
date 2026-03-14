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
        crate::logger::log("ANALYZER: Spawned successfully");

        let fft_size = 2048;
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_size);

        let mut input_buffer = Vec::with_capacity(fft_size);
        let mut scratch_buffer = vec![0.0f32; fft_size];
        let mut last_width = 0;
        let mut cached_indices: Vec<(usize, usize, f32)> = Vec::new();
        let sample_rate = 48000.0;

        let conf = crate::config::config::Config::global();
        let ui = &conf.ui;

        loop {
            // Ждем данные с таймаутом 200мс
            let msg = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await;

            match msg {
                Ok(Some(sample)) => {
                    input_buffer.push(sample);

                    // Выгребаем всё из канала пачкой
                    let mut count = 0;
                    while let Ok(s) = rx.try_recv() {
                        input_buffer.push(s);
                        count += 1;
                        if input_buffer.len() > fft_size * 4 {
                            break;
                        }
                    }

                    if count > fft_size {
                        crate::logger::log(&format!(
                            "ANALYZER: Large burst detected ({} samples)",
                            count
                        ));
                    }

                    if input_buffer.len() >= fft_size {
                        let target_width = if let Ok(out) = output.try_lock() {
                            out.len()
                        } else {
                            0
                        };

                        if target_width > 0 {
                            let max_amp = input_buffer
                                .iter()
                                .take(fft_size)
                                .fold(0.0f32, |m, x| m.max(x.abs()));

                            if max_amp > 0.001 {
                                // ЛОГ: Только когда реально считаем FFT
                                // crate::logger::log(&format!("ANALYZER: Processing FFT, amp: {:.4}", max_amp));

                                if target_width != last_width {
                                    crate::logger::log(&format!(
                                        "ANALYZER: Resize {} -> {}",
                                        last_width, target_width
                                    ));
                                    cached_indices.clear();
                                    let f_min = 20.0f32;
                                    let f_max = 15000.0f32;
                                    let ratio = f_max / f_min;
                                    let get_idx =
                                        |hz: f32| ((hz * fft_size as f32) / sample_rate) as usize;
                                    for i in 0..target_width {
                                        let pct_s = i as f32 / target_width as f32;
                                        let pct_e = (i + 1) as f32 / target_width as f32;
                                        let s_idx = get_idx(f_min * ratio.powf(pct_s));
                                        let e_idx =
                                            get_idx(f_min * ratio.powf(pct_e)).max(s_idx + 1);
                                        cached_indices.push((s_idx, e_idx, pct_s));
                                    }
                                    last_width = target_width;
                                }

                                scratch_buffer.copy_from_slice(&input_buffer[..fft_size]);
                                let mut out_spectrum = fft.make_output_vec();

                                if fft.process(&mut scratch_buffer, &mut out_spectrum).is_ok() {
                                    let mut current_freqs = vec![0.0; target_width];
                                    for (i, &(s_idx, e_idx, pct_s)) in
                                        cached_indices.iter().enumerate()
                                    {
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
                            } else {
                                // Тишина в звуке
                                if let Ok(mut out) = output.try_lock() {
                                    if out.iter().any(|&v| v > 0.0) {
                                        crate::logger::log(
                                            "ANALYZER: Silence detected, clearing bars",
                                        );
                                        out.fill(0.0);
                                    }
                                }
                            }
                        }
                        input_buffer.clear();
                    }
                }
                Ok(Option::None) => {
                    crate::logger::log("ANALYZER: Channel closed, exiting task");
                    break;
                }
                Err(_) => {
                    // Сюда заходим только на паузе/стопе
                    if let Ok(mut out) = output.try_lock() {
                        if out.iter().any(|&v| v > 0.0) {
                            crate::logger::log("ANALYZER: Timeout (No data), clearing bars");
                            out.fill(0.0);
                        }
                    }
                    input_buffer.clear();
                }
            }

            // Отдаем управление, чтобы не блокировать поток
            tokio::task::yield_now().await;
        }
    });
}
