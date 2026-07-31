use std::sync::{mpsc, Arc};

use realfft::RealFftPlanner;
use tokio::sync::Mutex;

use std::sync::mpsc::Receiver;

/// Параметры визуализации спектра (значения по умолчанию — из старого config.jsonc).
#[derive(Clone, Copy, Debug)]
pub struct VisualizerSettings {
    pub eq_low: f32,
    pub eq_mid: f32,
    pub eq_high: f32,
    pub sensitivity: f32,
    pub noise_gate: f32,
    pub exponent: f32,
}

impl Default for VisualizerSettings {
    fn default() -> Self {
        Self {
            eq_low: 0.01,
            eq_mid: 0.07,
            eq_high: 0.3,
            sensitivity: 0.3,
            noise_gate: 0.01,
            exponent: 1.9,
        }
    }
}

/// Запускает анализатор в отдельном блокирующем потоке.
/// Принимает синхронный Receiver и обновляет `output` (общий вектор для UI).
pub fn spawn_analyzer(rx: Receiver<f32>, output: Arc<Mutex<Vec<f32>>>, settings: VisualizerSettings) {
    std::thread::spawn(move || {
        let fft_size = 2048;
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_size);

        let mut input_buffer = Vec::with_capacity(fft_size * 4);
        let mut scratch_buffer = vec![0.0f32; fft_size];
        let mut last_width = 0;
        let mut cached_indices: Vec<(usize, usize, f32)> = Vec::new();
        let sample_rate = 48000.0;

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
                                            settings.eq_low
                                        } else if pct_s < 0.6 {
                                            settings.eq_mid
                                        } else {
                                            settings.eq_high
                                        };

                                        energy *= multiplier * settings.sensitivity;
                                        if energy < settings.noise_gate {
                                            energy = 0.0;
                                        }
                                        current_freqs[i] = energy.powf(settings.exponent).min(1.0);
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
                        out.fill(0.0);
                    }
                    input_buffer.clear();
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn analyzer_updates_output_on_samples() {
        let (tx, rx) = mpsc::channel::<f32>();
        let out = Arc::new(Mutex::new(vec![0.0f32; 64]));
        spawn_analyzer(rx, out.clone(), VisualizerSettings::default());

        // Один блок FFT синусоидой 440 Гц (громко, чтобы прошёл noise_gate)
        for i in 0..4096 {
            let t = i as f32 / 48000.0;
            let sample = (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5;
            tx.send(sample).unwrap();
        }
        drop(tx);

        // Ждём, пока поток обработает блок (макс. 3 сек)
        for _ in 0..150 {
            std::thread::sleep(Duration::from_millis(20));
            let guard = out.blocking_lock();
            if guard.iter().any(|&v| v > 0.0) {
                return; // успех — output обновлён
            }
        }
        panic!("analyzer не обновил output");
    }
}
