use super::commands::AudioCmd;
use super::source_factory;
use super::status::EngineStatus;

use std::num::NonZero;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc as tokio_mpsc, watch};

use rodio::Player;
use rodio::cpal::traits::HostTrait;
use rodio::stream::DeviceSinkBuilder;

pub struct AudioEngine {
    cmd_tx: tokio_mpsc::Sender<AudioCmd>,
    status_rx: watch::Receiver<EngineStatus>,
    shutdown_tx: watch::Sender<bool>,
    pub cava_data: Arc<Mutex<Vec<f32>>>,
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl AudioEngine {
    pub fn new(visualizer: Option<VisualizerSettings>) -> Self {
        let (cmd_tx, mut cmd_rx) = tokio_mpsc::channel::<AudioCmd>(64);
        let (status_tx, status_rx) = watch::channel(EngineStatus::default());
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

        let cava_data = Arc::new(Mutex::new(vec![0.0; 128]));

        // Синхронный канал для визуализации (только если она включена)
        let viz_tx = if let Some(settings) = visualizer {
            let (tx, rx) = mpsc::channel::<f32>();
            spawn_analyzer(rx, cava_data.clone(), settings);
            Some(tx)
        } else {
            None
        };

        let task_handle = tokio::spawn(async move {
            let host = rodio::cpal::default_host();
            let device = host
                .default_output_device()
                .expect("No output device found.");

            let mut stream = DeviceSinkBuilder::from_device(device)
                .expect("Failed to create SinkBuilder")
                .with_sample_rate(NonZero::new(48000).unwrap())
                .open_sink_or_fallback()
                .expect("System Error: Failed to open audio sink.");

            stream.log_on_drop(false);

            let player = Player::connect_new(stream.mixer());

            loop {
                tokio::select! {
                    // Команды от UI
                    Some(cmd) = cmd_rx.recv() => {
                        match cmd {
                            AudioCmd::Play { path, channels } => {
                                if let Some(src) = source_factory::open_source(&path, channels).await {
                                    player.stop();

                                    if let Some(tx) = &viz_tx {
                                        player.append(VisualizableSource {
                                            input: src,
                                            sender: tx.clone(),
                                        });
                                    } else {
                                        player.append(src);
                                    }

                                    player.play();
                                }
                            }
                            AudioCmd::Stop => {
                                player.stop();
                            }
                            AudioCmd::Pause => {
                                player.pause();
                            }
                            AudioCmd::Resume => {
                                player.play();
                            }
                            AudioCmd::Volume(v) => player.set_volume(v),
                            AudioCmd::Seek(d) => {
                                let _ = player.try_seek(d);
                            }
                        }
                    }
                    // Сигнал завершения
                    Ok(()) = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            player.stop();
                            break;
                        }
                    }
                    // Периодическое обновление статуса
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        let _ = status_tx.send(EngineStatus {
                            position: player.get_pos(),
                            is_paused: player.is_paused(),
                            volume: player.volume(),
                            is_empty: player.empty(),
                        });
                    }
                }
            }
            // Даём время на освобождение аудиоустройства
            tokio::time::sleep(Duration::from_millis(50)).await;
        });

        Self {
            cmd_tx,
            status_rx,
            shutdown_tx,
            cava_data,
            task_handle: Some(task_handle),
        }
    }

    // --- ПУБЛИЧНЫЕ МЕТОДЫ ---

    pub async fn play(&self, path: &str) {
        let _ = self
            .cmd_tx
            .send(AudioCmd::Play {
                path: path.to_string(),
                channels: 2,
            })
            .await;
    }

    pub async fn stop(&self) {
        let _ = self.cmd_tx.send(AudioCmd::Stop).await;
    }

    pub async fn pause(&self) {
        let _ = self.cmd_tx.send(AudioCmd::Pause).await;
    }

    pub async fn resume(&self) {
        let _ = self.cmd_tx.send(AudioCmd::Resume).await;
    }

    pub fn is_paused(&self) -> bool {
        self.status_rx.borrow().is_paused
    }
    pub fn is_empty(&self) -> bool {
        self.status_rx.borrow().is_empty
    }
    pub fn get_current_pos(&self) -> u64 {
        self.status_rx.borrow().position.as_secs()
    }

    pub fn get_volume(&self) -> f32 {
        gain_to_volume(self.status_rx.borrow().volume)
    }

    pub async fn set_volume(&self, vol: f32) {
        let _ = self
            .cmd_tx
            .send(AudioCmd::Volume(volume_to_gain(vol)))
            .await;
    }

    pub async fn seek_to(&self, seconds: u64) {
        let _ = self
            .cmd_tx
            .send(AudioCmd::Seek(Duration::from_secs(seconds)))
            .await;
    }

    pub async fn seek_relative(&self, offset_secs: i64) {
        let current = self.status_rx.borrow().position.as_secs_f64();
        let target = Duration::from_secs_f64((current + offset_secs as f64).max(0.0));
        let _ = self.cmd_tx.send(AudioCmd::Seek(target)).await;
    }

    /// Сигнализирует движку о завершении работы и ждёт его остановки.
    pub async fn shutdown(&mut self) {
        let _ = self.shutdown_tx.send(true);
        if let Some(handle) = self.task_handle.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
        }
    }
}

/// Конвертирует gain (0..1 у rodio) в «человеческий» уровень громкости 0..1.
pub fn gain_to_volume(gain: f32) -> f32 {
    if gain <= 0.0 {
        return 0.0;
    }
    (f32::ln(gain * (f32::exp(5.0) - 1.0) + 1.0) / 5.0).clamp(0.0, 1.0)
}

/// Конвертирует «человеческий» уровень громкости 0..1 в gain для rodio.
pub fn volume_to_gain(vol: f32) -> f32 {
    let v = vol.clamp(0.0, 1.0);
    (f32::exp(v * 5.0) - 1.0) / (f32::exp(5.0) - 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_to_volume_zero() {
        assert_eq!(gain_to_volume(0.0), 0.0);
        assert_eq!(gain_to_volume(-1.0), 0.0);
    }

    #[test]
    fn gain_to_volume_max() {
        assert!((gain_to_volume(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn volume_round_trip() {
        for vol in [0.0, 0.1, 0.5, 0.9, 1.0] {
            let gain = volume_to_gain(vol);
            let back = gain_to_volume(gain);
            assert!((back - vol).abs() < 1e-6, "vol={} back={}", vol, back);
        }
    }

    #[test]
    fn volume_monotonic() {
        let mut prev = gain_to_volume(0.0);
        for g in 0..=100 {
            let v = gain_to_volume(g as f32 / 100.0);
            assert!(v >= prev - 1e-9);
            prev = v;
        }
    }
}
