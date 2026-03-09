use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::logger;

#[allow(unused_imports)]
use rodio::{Decoder, Source, Sink, OutputStream, OutputStreamHandle};

use lofty::prelude::*;
use lofty::probe::Probe;

pub struct AudioEngine {
    _stream: OutputStream,
    handle: OutputStreamHandle,
    sink: Arc<Mutex<Sink>>,
}

impl AudioEngine {
    pub fn new() -> Self {
        logger::log("System: Init High-Performance Audio Engine...");

        let (stream, handle) = OutputStream::try_default()
            .expect("Ошибка ALSA: Не удалось найти устройство вывода. Проверь драйверы.");
        let sink = Sink::try_new(&handle).expect("Не удалось инициализировать Sink.");

        Self {
            _stream: stream,
            handle,
            sink: Arc::new(Mutex::new(sink)),
        }
    }

    pub async fn play(&self, path: &str) -> (String, String) {
        let path_str = path.to_string();
        let handle = self.handle.clone();
        let sink_lock = self.sink.clone();

        let (artist, title) = self.extract_metadata(&path_str);

        tokio::spawn(async move {
            let source = tokio::task::spawn_blocking(move || {
                let file = File::open(&path_str).ok()?;
                Decoder::new(BufReader::new(file)).ok()
            }).await.unwrap_or(None);

            if let Some(src) = source {
                let mut sink = sink_lock.lock().await;
                if let Ok(new_sink) = Sink::try_new(&handle) {
                    new_sink.append(src);
                    *sink = new_sink;
                }
            }
        });

        (artist, title)
    }

    fn extract_metadata(&self, path: &str) -> (String, String) {
        let mut artist = "Unknown Artist".to_string();
        let mut title = "Unknown Track".to_string();

        if let Ok(tagged_file) = Probe::open(path).and_then(|p| p.read()) {
            let tag = tagged_file.primary_tag().or_else(|| tagged_file.first_tag());
            if let Some(t) = tag {
                artist = t.artist().map(|s| s.to_string()).unwrap_or(artist);
                title = t.title().map(|s| s.to_string()).unwrap_or(title);
            }
        }

        if title == "Unknown Track" {
            title = path.split('/').last().unwrap_or(path).to_string();
        }

        (artist, title)
    }

    pub async fn is_empty(&self) -> bool {
        let sink = self.sink.lock().await;
        sink.empty()
    }
}