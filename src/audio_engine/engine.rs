use super::opus_source::OpusSource;
use super::symphonia_source::SymphoniaSource;
use crate::audio_engine::visualizer::{VisualizableSource, spawn_analyzer};
use crate::logger;

use std::fs::File;
use std::io::BufReader;
use std::num::NonZero;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc, watch};

use lofty::prelude::*;
use lofty::probe::Probe;
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
    cmd_tx: mpsc::Sender<AudioCmd>,
    status_rx: watch::Receiver<EngineStatus>,
    pub cava_data: Arc<Mutex<Vec<f32>>>,
}

impl AudioEngine {
    pub fn new() -> (Self, mpsc::Receiver<f32>) {
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<AudioCmd>(64);
        let (status_tx, status_rx) = watch::channel(EngineStatus::default());
        let (viz_tx, viz_rx) = mpsc::channel(1024 * 10);
        let (_dummy_tx, dummy_rx) = mpsc::channel(1);

        let cava_data = Arc::new(Mutex::new(vec![0.0; 128]));
        let cava_inner = cava_data.clone();

        tokio::spawn(async move {
            let host = rodio::cpal::default_host();
            let device = host
                .default_output_device()
                .expect("No output device found.");

            // Заменяем name() на description() по просьбе компилятора
            if let Ok(desc) = device.description() {
                logger::log(&format!("ENGINE: Using device {}", desc));
            }

            let mut stream = DeviceSinkBuilder::from_device(device)
                .expect("Failed to create SinkBuilder")
                .with_sample_rate(NonZero::new(48000).unwrap())
                .open_sink_or_fallback()
                .expect("System Error: Failed to open audio sink.");

            // 1. Говорим ему не орать при закрытии
            stream.log_on_drop(false);

            let player = Player::connect_new(stream.mixer());
            spawn_analyzer(viz_rx, cava_inner);

            loop {
                tokio::select! {
                    Some(cmd) = cmd_rx.recv() => {
                        match cmd {
                            AudioCmd::Play { path, channels } => {
                                logger::log(&format!("ENGINE: Loading track {}", path));
                                if let Some(src) = Self::prepare_source(&path, channels).await {
                                    player.stop();
                                    player.append(VisualizableSource {
                                        input: src,
                                        sender: viz_tx.clone(),
                                    });
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
        });

        (
            Self {
                cmd_tx,
                status_rx,
                cava_data,
            },
            dummy_rx,
        )
    }

    async fn prepare_source(path: &str, channels: u16) -> Option<Box<dyn Source + Send>> {
        let p = path.to_string();
        tokio::task::spawn_blocking(move || {
            let file = File::open(&p).ok()?;
            if p.to_lowercase().ends_with(".opus") {
                return OpusSource::new(BufReader::new(file), channels)
                    .map(|s| Box::new(s) as Box<dyn Source + Send>);
            }
            SymphoniaSource::new(file).map(|s| Box::new(s) as Box<dyn Source + Send>)
        })
        .await
        .ok()?
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

    async fn get_audio_info(&self, path: &str) -> (String, String, u32, u16) {
        let path_owned = path.to_string();
        tokio::task::spawn_blocking(move || {
            let mut info = ("Unknown".to_string(), "Unknown".to_string(), 48000, 2);
            if let Ok(probe) = Probe::open(&path_owned) {
                if let Ok(tagged_file) = probe.read() {
                    let props = tagged_file.properties();
                    let sample_rate = props.sample_rate().unwrap_or(48000);
                    let channels = props.channels().map(|c| c as u16).unwrap_or(2);

                    if let Some(t) = tagged_file
                        .primary_tag()
                        .or_else(|| tagged_file.first_tag())
                    {
                        let rt = tokio::runtime::Handle::current();
                        let (artist, title) =
                            rt.block_on(crate::parser::artist::process_and_log_metadata(
                                t.artist().map(|s| s.to_string()),
                                t.title().map(|s| s.to_string()),
                                t.album().map(|s| s.to_string()),
                                t.get_string(ItemKey::Year).map(|s| s.to_string()),
                                t.genre().map(|s| s.to_string()),
                                t.get_string(ItemKey::Comment).map(|s| s.to_string()),
                            ));
                        info = (artist, title, sample_rate, channels);
                    } else {
                        info = ("Unknown".into(), "Unknown".into(), sample_rate, channels);
                    }
                }
            }
            info
        })
        .await
        .unwrap_or(("Unknown".to_string(), "Unknown".to_string(), 48000, 2))
    }
}
