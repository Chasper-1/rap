use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::sync::Arc;
use std::num::NonZero;
use std::time::Duration;
use tokio::sync::Mutex;
use crate::logger;

// rodio 0.22.2
use rodio::{Decoder, Player, Source};
use rodio::stream::{DeviceSinkBuilder, MixerDeviceSink};

// ИСПРАВЛЕНИЕ: Импортируем строгие типы SampleRate и Channels для opus-codec 0.1.2
use opus_codec::{Decoder as OpusDecoder, SampleRate, Channels}; 
use ogg::PacketReader;

use lofty::prelude::*;
use lofty::probe::Probe;

pub struct OpusSource<R: Read + Seek> {
    packet_reader: PacketReader<R>,
    decoder: OpusDecoder,
    sample_buffer: Vec<f32>,
    buffer_pos: usize,
    sample_rate: u32,
    channels: u16,
}

impl<R: Read + Seek> OpusSource<R> {
    pub fn new(reader: R, channels: u16) -> Option<Self> {
        // ИСПРАВЛЕНИЕ E0308: Передаем спец-типы вместо i32/usize
        let rate = SampleRate::Hz48000;
        let chans = if channels == 1 { Channels::Mono } else { Channels::Stereo };

        let decoder = OpusDecoder::new(rate, chans).ok()?;
        let packet_reader = PacketReader::new(reader);

        Some(Self {
            packet_reader,
            decoder,
            sample_buffer: Vec::new(),
            buffer_pos: 0,
            sample_rate: 48000,
            channels,
        })
    }
}

impl<R: Read + Seek> Iterator for OpusSource<R> {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buffer_pos >= self.sample_buffer.len() {
            loop {
                match self.packet_reader.read_packet() {
                    Ok(Some(packet)) => {
                        if packet.data.starts_with(b"OpusHead") || packet.data.starts_with(b"OpusTags") {
                            continue;
                        }

                        let mut pcm_buf = vec![0.0f32; 5760 * self.channels as usize];
                        // ИСПРАВЛЕНИЕ E0308: Передаем &[u8] напрямую, а не Option
                        if let Ok(decoded_size) = self.decoder.decode_float(&packet.data, &mut pcm_buf, false) {
                            self.sample_buffer = pcm_buf[..decoded_size * self.channels as usize].to_vec();
                            self.buffer_pos = 0;
                            break;
                        }
                    }
                    _ => return None,
                }
            }
        }

        let sample = self.sample_buffer.get(self.buffer_pos).cloned();
        self.buffer_pos += 1;
        sample
    }
}

impl<R: Read + Seek> Source for OpusSource<R> {
    fn current_span_len(&self) -> Option<usize> { None }
    fn channels(&self) -> NonZero<u16> { NonZero::new(self.channels).unwrap_or(NonZero::new(2).unwrap()) }
    fn sample_rate(&self) -> NonZero<u32> { NonZero::new(self.sample_rate).unwrap_or(NonZero::new(48000).unwrap()) }
    fn total_duration(&self) -> Option<Duration> { None }
}

pub struct AudioEngine {
    _stream: MixerDeviceSink,
    player: Arc<Mutex<Player>>,
}

impl AudioEngine {
    pub fn new() -> Self {
        logger::log("System: Init Audio Engine...");
        let mut stream = DeviceSinkBuilder::open_default_sink()
            .expect("Ошибка: Устройство вывода не найдено.");
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
                let reader = BufReader::new(file);

                if path_str.to_lowercase().ends_with(".opus") {
                    OpusSource::new(reader, channels)
                        .map(|s| Box::new(s) as Box<dyn Source + Send>)
                } else {
                    Decoder::new(reader).ok()
                        .map(|d| Box::new(d) as Box<dyn Source + Send>)
                }
            }).await;

            if let Ok(Some(src)) = source_result {
                let player = player_lock.lock().await;
                player.append(src);
            }
        });
        (artist, title)
    }

    async fn get_audio_info(&self, path: &str) -> (String, String, u32, u16) {
        let mut info = ("Unknown".to_string(), "Unknown".to_string(), 48000, 2);
        
        if let Ok(probe) = Probe::open(path) {
            if let Ok(tagged_file) = probe.read() {
                let tag = tagged_file.primary_tag().or_else(|| tagged_file.first_tag());
                
                if let Some(t) = tag {
                    let raw_artist = t.artist().map(|s| s.to_string());
                    let title = t.title().map(|s| s.to_string());
                    let album = t.album().map(|s| s.to_string());
                    let year = t.get_string(lofty::prelude::ItemKey::Year)
                                .or_else(|| t.get_string(lofty::prelude::ItemKey::RecordingDate))
                                .map(|s| s.to_string());
                    let genre = t.genre().map(|s| s.to_string());
                    let comment = t.get_string(lofty::prelude::ItemKey::Comment).map(|s| s.to_string());
                
                    // Теперь получаем ровно столько, сколько используем
                    let (artist, title_final) = crate::parser::artist::process_and_log_metadata(
                        raw_artist, title, album, year, genre, comment
                    ).await;
                
                    info.0 = artist;
                    info.1 = title_final;
                }
                
                let props = tagged_file.properties();
                info.2 = props.sample_rate().unwrap_or(48000);
                info.3 = props.channels().map(|c| c as u16).unwrap_or(2);
            }
        }
        info
    }

    pub async fn is_empty(&self) -> bool {
        self.player.lock().await.empty()
    }
}