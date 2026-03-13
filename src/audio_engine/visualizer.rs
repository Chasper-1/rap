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

        loop {
            // Берем твой родной fall_speed из конфига
            let fall_speed = crate::config::config::Config::global().ui.cava_fall_speed;

            match tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await {
                Ok(Some(sample)) => {
                    input_buffer.push(sample);

                    if input_buffer.len() >= fft_size {
                        let mut out_spectrum = fft.make_output_vec();
                        let mut indata = input_buffer.clone();

                        if fft.process(&mut indata, &mut out_spectrum).is_ok() {
                            let mut current_freqs = vec![0.0; 128];
                            let useful_bins = fft_size / 4; // Ограничение ~12кГц
                            let bin_per_band = (useful_bins / 128).max(1);

                            for i in 0..128 {
                                let mut amp = 0.0;
                                for j in 0..bin_per_band {
                                    let idx = i * bin_per_band + j;
                                    if idx < out_spectrum.len() {
                                        amp += out_spectrum[idx].norm();
                                    }
                                }
                                amp /= bin_per_band as f32;

                                let mut val = (amp * 25.0).log10().max(0.0) / 2.2;
                                
                                // Падение во время игры
                                val = val.max(prev_freqs[i] * fall_speed);
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
                    // ТАЙМ-АУТ (ПАУЗА) — используем ту же скорость падения
                    let mut current_freqs = vec![0.0; 128];
                    let mut has_energy = false;

                    for i in 0..128 {
                        current_freqs[i] = prev_freqs[i] * fall_speed;
                        if current_freqs[i] > 0.001 {
                            has_energy = true;
                        } else {
                            current_freqs[i] = 0.0;
                        }
                    }

                    prev_freqs = current_freqs.clone();
                    if let Ok(mut out) = output.try_lock() {
                        *out = current_freqs;
                    }

                    input_buffer.clear();
                    if !has_energy {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                }
            }
        }
    });
}