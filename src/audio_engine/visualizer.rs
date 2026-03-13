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
                                // Делим экран на три равные части
                                let one_third = target_width / 3;
                                let two_thirds = (target_width * 2) / 3;

                                let (start_hz, end_hz, zone_sens) = if i < one_third {
                                    // --- БАСС (Первая треть) ---
                                    let pct_s = i as f32 / one_third as f32;
                                    let pct_e = (i + 1) as f32 / one_third as f32;
                                    // Расширенный бас до 800Гц, чтобы занять пустоту
                                    (
                                        30.0 + 770.0 * pct_s.powf(1.5),
                                        30.0 + 770.0 * pct_e.powf(1.5),
                                        ui.cava_sensitivity_low,
                                    )
                                } else if i < two_thirds {
                                    // --- СЕРЕДИНА (Вторая треть) ---
                                    let pct_s =
                                        (i - one_third) as f32 / (two_thirds - one_third) as f32;
                                    let pct_e = (i - one_third + 1) as f32
                                        / (two_thirds - one_third) as f32;
                                    (
                                        800.0 + 4200.0 * pct_s,
                                        800.0 + 4200.0 * pct_e,
                                        ui.cava_sensitivity_mid,
                                    )
                                } else {
                                    // --- ВЕРХА (Последняя треть) ---
                                    let pct_s = (i - two_thirds) as f32
                                        / (target_width - two_thirds) as f32;
                                    let pct_e = (i - two_thirds + 1) as f32
                                        / (target_width - two_thirds) as f32;
                                    (
                                        5000.0 + 13000.0 * pct_s,
                                        5000.0 + 13000.0 * pct_e,
                                        ui.cava_sensitivity_high,
                                    )
                                };

                                let s_idx = get_idx(start_hz);
                                let e_idx = get_idx(end_hz).max(s_idx + 1);

                                let mut amp = 0.0;
                                let chunk = &out_spectrum[s_idx..e_idx.min(out_spectrum.len())];

                                if !chunk.is_empty() {
                                    // Берем пиковое значение для максимальной отзывчивости каждого столбика
                                    for bin in chunk {
                                        let n = bin.norm();
                                        if n > amp {
                                            amp = n;
                                        }
                                    }
                                }

                                let mut val =
                                    (if amp < ui.cava_noise_gate { 0.0 } else { amp }) * zone_sens;
                                val *= 1.0 + (ui.cava_tilt * (i as f32 / target_width as f32));
                                val = val.tanh().powf(ui.cava_exponent);

                                let prev = prev_freqs[i];
                                if val > prev {
                                    val = prev + (val - prev) * ui.cava_attack;
                                } else {
                                    val = (prev * ui.cava_fall_speed).max(val);
                                }

                                if val < 0.001 {
                                    val = 0.0;
                                }
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
