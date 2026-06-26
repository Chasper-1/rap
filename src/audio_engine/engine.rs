use super::source_factory;
use crate::audio_engine::visualizer::{VisualizableSource, spawn_analyzer};
use crate::logger;

use std::num::NonZero;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc as tokio_mpsc, watch};

use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::stream::DeviceSinkBuilder;
use rodio::{Player, Source};

pub enum AudioCmd {
    Play { path: String, channels: u16 },
    Stop,
    Pause,
    Resume,
    Seek(Duration),
    Volume(f32),
}

#[derive(Clone, Default)]
pub struct EngineStatus {
    pub position: Duration,
    pub is_paused: bool,
    pub volume: f32,
    pub is_empty: bool,
}

pub struct AudioEngine {
    cmd_tx: tokio_mpsc::Sender<AudioCmd>,
    status_rx: watch::Receiver<EngineStatus>,
    shutdown_tx: watch::Sender<bool>,
    pub cava_data: Arc<Mutex<Vec<f32>>>,
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl AudioEngine {
    pub fn new() -> Self {
        let (cmd_tx, mut cmd_rx) = tokio_mpsc::channel::<AudioCmd>(64);
        let (status_tx, status_rx) = watch::channel(EngineStatus::default());
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

        // Синхронный канал для визуализации
        let (viz_tx, viz_rx) = mpsc::channel::<f32>();

        let cava_data = Arc::new(Mutex::new(vec![0.0; 128]));
        let cava_inner = cava_data.clone();

        // Запускаем анализатор в отдельном потоке ОС
        if crate::config::config::Config::global().ui.cava_show {
            spawn_analyzer(viz_rx, cava_inner);
        }

        let task_handle = tokio::spawn(async move {
            let host = rodio::cpal::default_host();
            let device = host
                .default_output_device()
                .expect("No output device found.");

            if let Ok(desc) = device.description() {
                logger::log(&format!("ENGINE: Using device {}", desc));
            }

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
                                logger::log(&format!("ENGINE: Loading track {}", path));
                                if let Some(src) = source_factory::open_source(&path, channels).await {
                                    player.stop();

                                    // Проверяем настройку из конфига
                                    if crate::config::config::Config::global().ui.cava_show {
                                        // Если CAVA включен, оборачиваем в визуализатор
                                        player.append(VisualizableSource {
                                            input: src,
                                            sender: viz_tx.clone(),
                                        });
                                    } else {
                                        // Если CAVA выключен, бросаем чистый источник напрямую
                                        player.append(src);
                                    }

                                    player.play();
                                    logger::log("ENGINE: Playback started");
                                }
                            }
                            AudioCmd::Stop => {
                                logger::log("ENGINE: Stop");
                                player.stop();
                            }
                            AudioCmd::Pause => {
                                logger::log("ENGINE: Pause");
                                player.pause();
                            }
                            AudioCmd::Resume => {
                                logger::log("ENGINE: Resume");
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
                            logger::log("ENGINE: Shutdown signal received");
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

    pub async fn play(&self, path: &str) -> (String, String) {
        let (artist, title, _, channels) = self.get_audio_info(path).await;
        let _ = self
            .cmd_tx
            .send(AudioCmd::Play {
                path: path.to_string(),
                channels,
            })
            .await;
        (artist, title)
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
        let gain = self.status_rx.borrow().volume;
        if gain <= 0.0 {
            return 0.0;
        }
        (f32::ln(gain * (f32::exp(5.0) - 1.0) + 1.0) / 5.0).clamp(0.0, 1.0)
    }

    pub async fn set_volume(&self, vol: f32) {
        let v = vol.clamp(0.0, 1.0);
        let calibrated = (f32::exp(v * 5.0) - 1.0) / (f32::exp(5.0) - 1.0);
        let _ = self.cmd_tx.send(AudioCmd::Volume(calibrated)).await;
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

    async fn get_audio_info(&self, _path: &str) -> (String, String, u32, u16) {
        ("Unknown".to_string(), "Unknown".to_string(), 48000, 2)
    }
}