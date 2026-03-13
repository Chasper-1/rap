use super::opus_source::OpusSource;
use super::symphonia_source::SymphoniaSource;
use crate::logger;

use std::fs::File;
use std::io::BufReader;
use std::num::NonZero;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use rodio::cpal::traits::HostTrait;
use rodio::stream::{DeviceSinkBuilder, MixerDeviceSink};
use rodio::{Player, Source};
use lofty::prelude::*;
use lofty::probe::Probe;

pub struct AudioEngine {
    _stream: MixerDeviceSink,
    player: Arc<Mutex<Player>>,
}

impl AudioEngine {
    pub fn new() -> Self {
        let device = rodio::cpal::default_host()
            .default_output_device()
            .expect("System Error: No output device found.");

        // Исправлено: добавлена развёртка Result после from_device
        let mut stream = DeviceSinkBuilder::from_device(device)
            .expect("Failed to create SinkBuilder")
            .with_sample_rate(NonZero::new(48000).unwrap())
            .open_sink_or_fallback()
            .expect("System Error: Failed to open audio sink.");

        stream.log_on_drop(false);
        let player = Player::connect_new(stream.mixer());

        Self {
            _stream: stream,
            player: Arc::new(Mutex::new(player)),
        }
    }

    pub async fn play(&self, path: &str) -> (String, String) {
        let path_str = path.to_string();
        let player_lock = self.player.clone();
        let (artist, title, _, channels) = self.get_audio_info(&path_str).await;

        tokio::spawn(async move {
            let source_result = tokio::task::spawn_blocking(move || -> Option<Box<dyn Source + Send>> {
                let file = File::open(&path_str).ok()?;
                let ext = path_str.to_lowercase();
                
                if ext.ends_with(".opus") {
                    return OpusSource::new(BufReader::new(file), channels)
                        .map(|s| Box::new(s) as Box<dyn Source + Send>);
                }

                SymphoniaSource::new(file).map(|s| Box::new(s) as Box<dyn Source + Send>)
            }).await;

            if let Ok(Some(src)) = source_result {
                let p = player_lock.lock().await;
                p.stop();
                p.append(src);
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

    pub async fn pause(&self) { self.player.lock().await.pause(); }
    pub async fn resume(&self) { self.player.lock().await.play(); }
    pub async fn set_volume(&self, vol: f32) { self.player.lock().await.set_volume(vol.clamp(0.0, 1.0)); }
    pub async fn get_volume(&self) -> f32 { self.player.lock().await.volume() }
    pub async fn get_current_pos(&self) -> u64 { self.player.lock().await.get_pos().as_secs() }
    pub async fn is_paused(&self) -> bool { self.player.lock().await.is_paused() }

    async fn get_audio_info(&self, path: &str) -> (String, String, u32, u16) {
        let mut info = ("Unknown".to_string(), "Unknown".to_string(), 48000, 2);
        if let Ok(probe) = Probe::open(path) {
            if let Ok(tagged_file) = probe.read() {
                if let Some(t) = tagged_file.primary_tag().or_else(|| tagged_file.first_tag()) {
                    let (artist, title) = crate::parser::artist::process_and_log_metadata(
                        t.artist().map(|s| s.to_string()),
                        t.title().map(|s| s.to_string()),
                        t.album().map(|s| s.to_string()),
                        t.get_string(ItemKey::Year).map(|s| s.to_string()),
                        t.genre().map(|s| s.to_string()),
                        t.get_string(ItemKey::Comment).map(|s| s.to_string()),
                    ).await;
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