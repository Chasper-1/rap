use super::opus_source::OpusSource;
use super::symphonia_source::SymphoniaSource;
use crate::logger;

use crate::audio_engine::visualizer::{VisualizableSource, spawn_analyzer};
use std::fs::File;
use std::io::BufReader;
use std::num::NonZero;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

use lofty::prelude::*;
use lofty::probe::Probe;
use rodio::cpal::traits::{DeviceTrait, HostTrait}; // Добавил DeviceTrait
use rodio::stream::{DeviceSinkBuilder, MixerDeviceSink};
use rodio::{Player, Source};

pub struct AudioEngine {
    _stream: MixerDeviceSink,
    player: Arc<Mutex<Player>>,
    viz_tx: mpsc::Sender<f32>,
    pub cava_data: Arc<Mutex<Vec<f32>>>,
}

impl AudioEngine {
    pub fn new() -> (Self, mpsc::Receiver<f32>) {
        let (tx, rx) = mpsc::channel(1024 * 10);
        let cava_data = Arc::new(Mutex::new(vec![0.0; 128]));

        let device = rodio::cpal::default_host()
            .default_output_device()
            .expect("System Error: No output device found.");

        // Лог девайса
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

        spawn_analyzer(rx, cava_data.clone());

        let engine = Self {
            _stream: stream,
            player: Arc::new(Mutex::new(player)),
            viz_tx: tx,
            cava_data,
        };

        let (_, dummy_rx) = mpsc::channel(1);
        (engine, dummy_rx)
    }

    pub async fn play(&self, path: &str) -> (String, String) {
        let path_str = path.to_string();
        let player_lock = self.player.clone();

        logger::log(&format!("ENGINE: Loading track {}", path));
        let (artist, title, _, channels) = self.get_audio_info(&path_str).await;
        let viz_tx = self.viz_tx.clone();

        tokio::spawn(async move {
            let source_result =
                tokio::task::spawn_blocking(move || -> Option<Box<dyn Source + Send>> {
                    let file = File::open(&path_str).ok()?;
                    let ext = path_str.to_lowercase();

                    if ext.ends_with(".opus") {
                        return OpusSource::new(BufReader::new(file), channels)
                            .map(|s| Box::new(s) as Box<dyn Source + Send>);
                    }

                    SymphoniaSource::new(file).map(|s| Box::new(s) as Box<dyn Source + Send>)
                })
                .await;

            if let Ok(Some(src)) = source_result {
                let p = player_lock.lock().await;
                p.stop();

                let visual_src = VisualizableSource {
                    input: src,
                    sender: viz_tx.clone(),
                };

                p.append(visual_src);
                p.play();
                logger::log("ENGINE: Playback started");
            }
        });

        (artist, title)
    }

    pub async fn is_empty(&self) -> bool {
        self.player.lock().await.empty()
    }

    pub async fn seek_to(&self, seconds: u64) {
        let p = self.player.lock().await;
        let _ = p.try_seek(Duration::from_secs(seconds));
    }

    pub async fn seek_relative(&self, offset_secs: i64) {
        let p = self.player.lock().await;
        let current_pos = p.get_pos();
        let target_secs = (current_pos.as_secs_f64() + offset_secs as f64).max(0.0);
        let target_duration = Duration::from_secs_f64(target_secs);

        if p.try_seek(target_duration).is_ok() {
            p.play();
            logger::log(&format!("AUDIO: Jumped to {:.1}s", target_secs));
        }
    }

    pub async fn pause(&self) {
        logger::log("ENGINE: Pause");
        self.player.lock().await.pause();
    }

    pub async fn resume(&self) {
        logger::log("ENGINE: Resume");
        self.player.lock().await.play();
    }

    pub async fn set_volume(&self, vol: f32) {
        let v = vol.clamp(0.0, 1.0);
        let calibrated = (f32::exp(v * 5.0) - 1.0) / (f32::exp(5.0) - 1.0);
        self.player.lock().await.set_volume(calibrated);
    }

    pub async fn get_volume(&self) -> f32 {
        let gain = self.player.lock().await.volume();
        if gain <= 0.0 {
            return 0.0;
        }
        (f32::ln(gain * (f32::exp(5.0) - 1.0) + 1.0) / 5.0).clamp(0.0, 1.0)
    }

    pub async fn get_current_pos(&self) -> u64 {
        self.player.lock().await.get_pos().as_secs()
    }

    pub async fn is_paused(&self) -> bool {
        self.player.lock().await.is_paused()
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
