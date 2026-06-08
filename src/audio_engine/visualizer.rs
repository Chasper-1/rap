use realfft::RealFftPlanner;
use rodio::Source;
use std::num::NonZero;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::Mutex;

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
        let sample = self.input.next();
        if let Some(ref s) = sample {
            // Вот эта проверка:
            if crate::config::config::Config::global().ui.cava_show {
                let clone: f32 = s.clone().into();
                let _ = self.sender.send(clone);
            }
        }
        sample
    }
}

/// Запускает анализатор в отдельном блокирующем потоке.
/// Принимает синхронный Receiver и обновляет `output` (общий вектор для UI).
pub fn spawn_analyzer(rx: Receiver<f32>, output: Arc<Mutex<Vec<f32>>>) {
    std::thread::spawn(move || {
        crate::logger::log("ANALYZER: Started in blocking thread");

        let fft_size = 2048;
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_size);

        let mut input_buffer = Vec::with_capacity(fft_size * 4);
        let mut scratch_buffer = vec![0.0f32; fft_size];
        let mut last_width = 0;
        let mut cached_indices: Vec<(usize, usize, f32)> = Vec::new();
        let sample_rate = 48000.0;

        let conf = crate::config::config::Config::global();
        let ui = &conf.ui;

        loop {
            // Ждём данные с таймаутом 100 мс
            match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(sample) => {
                    input_buffer.push(sample);
                    // Выгребаем всё, что накопилось
                    while let Ok(s) = rx.try_recv() {
                        input_buffer.push(s);
                        if input_buffer.len() > fft_size * 4 {
                            break;
                        }
                    }

                    while input_buffer.len() >= fft_size {
                        let target_width = {
                            // Синхронно блокируем мьютекс (tokio::sync::Mutex в синхронном коде)
                            let out = output.blocking_lock();
                            out.len()
                        };

                        if target_width > 0 {
                            scratch_buffer.copy_from_slice(&input_buffer[..fft_size]);
                            let max_amp = scratch_buffer.iter().fold(0.0f32, |m, x| m.max(x.abs()));

                            if max_amp > 0.001 {
                                // Кэширование индексов частот
                                if target_width != last_width {
                                    cached_indices.clear();
                                    let f_min = 20.0f32;
                                    let f_max = 13000.0f32;
                                    let ratio = f_max / f_min;
                                    let get_idx =
                                        |hz: f32| ((hz * fft_size as f32) / sample_rate) as usize;

                                    for i in 0..target_width {
                                        let pct_s = i as f32 / target_width as f32;
                                        let s_idx = get_idx(f_min * ratio.powf(pct_s));
                                        let e_idx = get_idx(
                                            f_min
                                                * ratio.powf((i + 1) as f32 / target_width as f32),
                                        )
                                        .max(s_idx + 1);
                                        cached_indices.push((s_idx, e_idx, pct_s));
                                    }
                                    last_width = target_width;
                                }

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
                                        }

                                        let multiplier = if pct_s < 0.2 {
                                            ui.eq_low
                                        } else if pct_s < 0.6 {
                                            ui.eq_mid
                                        } else {
                                            ui.eq_high
                                        };

                                        energy *= multiplier * ui.cava_sensitivity;
                                        if energy < ui.cava_noise_gate {
                                            energy = 0.0;
                                        }
                                        current_freqs[i] = energy.powf(ui.cava_exponent).min(1.0);
                                    }

                                    let mut out = output.blocking_lock();
                                    *out = current_freqs;
                                }
                            } else {
                                // Тишина – обнуляем
                                let mut out = output.blocking_lock();
                                if out.iter().any(|&v| v > 0.0) {
                                    out.fill(0.0);
                                }
                            }
                        }
                        input_buffer.drain(..fft_size);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Таймаут без данных – обнуляем визуализацию
                    let mut out = output.blocking_lock();
                    if out.iter().any(|&v| v > 0.0) {
                        crate::logger::log("ANALYZER: Timeout (silence), clearing bars");
                        out.fill(0.0);
                    }
                    input_buffer.clear();
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    crate::logger::log("ANALYZER: Sender dropped, exiting");
                    break;
                }
            }
        }
    });
}
