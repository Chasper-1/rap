use super::opus_source::OpusSource;
use super::symphonia_source::SymphoniaSource;
use crate::logger;

use crate::audio_engine::visualizer::VisualizableSource;
use std::fs::File;
use std::io::BufReader;
use std::num::NonZero;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

use lofty::prelude::*;
use lofty::probe::Probe;
use rodio::cpal::traits::HostTrait;
use rodio::stream::{DeviceSinkBuilder, MixerDeviceSink};
use rodio::{Player, Source};

pub struct AudioEngine {
    _stream: MixerDeviceSink,
    player: Arc<Mutex<Player>>,
    viz_tx: mpsc::Sender<f32>,
}

impl AudioEngine {
    pub fn new() -> (Self, mpsc::Receiver<f32>) {
        let (tx, rx) = mpsc::channel(1024 * 10);
    
        let device = rodio::cpal::default_host()
            .default_output_device()
            .expect("System Error: No output device found.");
    
        let mut stream = DeviceSinkBuilder::from_device(device)
            .expect("Failed to create SinkBuilder")
            .with_sample_rate(NonZero::new(48000).unwrap())
            .open_sink_or_fallback()
            .expect("System Error: Failed to open audio sink.");
    
        stream.log_on_drop(false);
        let player = Player::connect_new(stream.mixer());
    
        // 1. Создаем структуру и сохраняем в переменную
        let engine = Self {
            _stream: stream,
            player: Arc::new(Mutex::new(player)),
            viz_tx: tx,
        };
    
        // 2. Сразу возвращаем её и канал. Больше ничего создавать не надо!
        (engine, rx)
    }

    pub async fn play(&self, path: &str) -> (String, String) {
        let path_str = path.to_string();
        let player_lock = self.player.clone();
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
                let visualizable = VisualizableSource {
                    input: src,
                    sender: viz_tx,
                };

                let p = player_lock.lock().await;
                p.stop();
                p.append(visualizable);
                p.play();
            }
        });
        (artist, title)
    }

    // Исправлено: добавлен метод is_empty
    pub async fn is_empty(&self) -> bool {
        self.player.lock().await.empty()
    }

    // Исправлено: добавлен метод seek_to
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
        self.player.lock().await.pause();
    }
    pub async fn resume(&self) {
        self.player.lock().await.play();
    }
    pub async fn set_volume(&self, vol: f32) {
        self.player.lock().await.set_volume(vol.clamp(0.0, 1.0));
    }
    pub async fn get_volume(&self) -> f32 {
        self.player.lock().await.volume()
    }
    pub async fn get_current_pos(&self) -> u64 {
        self.player.lock().await.get_pos().as_secs()
    }
    pub async fn is_paused(&self) -> bool {
        self.player.lock().await.is_paused()
    }

    async fn get_audio_info(&self, path: &str) -> (String, String, u32, u16) {
        let mut info = ("Unknown".to_string(), "Unknown".to_string(), 48000, 2);
        if let Ok(probe) = Probe::open(path) {
            if let Ok(tagged_file) = probe.read() {
                if let Some(t) = tagged_file
                    .primary_tag()
                    .or_else(|| tagged_file.first_tag())
                {
                    let (artist, title) = crate::parser::artist::process_and_log_metadata(
                        t.artist().map(|s| s.to_string()),
                        t.title().map(|s| s.to_string()),
                        t.album().map(|s| s.to_string()),
                        t.get_string(ItemKey::Year).map(|s| s.to_string()),
                        t.genre().map(|s| s.to_string()),
                        t.get_string(ItemKey::Comment).map(|s| s.to_string()),
                    )
                    .await;
                    info.0 = artist;
                    info.1 = title;
                }
                let props = tagged_file.properties();
                info.2 = props.sample_rate().unwrap_or(48000);
                info.3 = props.channels().map(|c| c as u16).unwrap_or(2);
            }
        }
        info
    }
}
