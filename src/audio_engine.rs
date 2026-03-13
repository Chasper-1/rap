use crate::logger;
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::num::NonZero;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

// rodio 0.22.2
use rodio::cpal::traits::HostTrait;
use rodio::stream::{DeviceSinkBuilder, MixerDeviceSink};
use rodio::{Decoder, Player, Source};

use ogg::PacketReader;
use opus_codec::{Channels, Decoder as OpusDecoder, SampleRate as OpusSampleRate};

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
        let rate = OpusSampleRate::Hz48000;
        let chans = if channels == 1 {
            Channels::Mono
        } else {
            Channels::Stereo
        };

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
                        if packet.data.starts_with(b"OpusHead")
                            || packet.data.starts_with(b"OpusTags")
                        {
                            continue;
                        }

                        let mut pcm_buf = vec![0.0f32; 5760 * self.channels as usize];
                        if let Ok(decoded_size) =
                            self.decoder.decode_float(&packet.data, &mut pcm_buf, false)
                        {
                            self.sample_buffer =
                                pcm_buf[..decoded_size * self.channels as usize].to_vec();
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

impl<R: Read + Seek + Send> Source for OpusSource<R> {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> NonZero<u16> {
        NonZero::new(self.channels).unwrap_or(NonZero::new(2).unwrap())
    }
    fn sample_rate(&self) -> NonZero<u32> {
        NonZero::new(self.sample_rate).unwrap_or(NonZero::new(48000).unwrap())
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        let granule = (pos.as_secs_f64() * 48000.0) as u64;
        // В ogg 0.9 метод называется seek_absgp
        if self.packet_reader.seek_absgp(None, granule).is_ok() {
            self.sample_buffer.clear();
            self.buffer_pos = 0;
            Ok(())
        } else {
            Err(rodio::source::SeekError::NotSupported {
                underlying_source: "OpusSource",
            })
        }
    }
}

pub struct AudioEngine {
    _stream: MixerDeviceSink,
    player: Arc<Mutex<Player>>,
    _total_duration: Arc<Mutex<Option<Duration>>>,
}

impl AudioEngine {
    pub fn new() -> Self {
        logger::log("System: Init Audio Engine...");

        let device = rodio::cpal::default_host()
            .default_output_device()
            .expect("Ошибка: Устройство вывода не найдено.");

        let mut stream = DeviceSinkBuilder::from_device(device)
            .expect("Ошибка: Не удалось создать билдер.")
            .with_sample_rate(std::num::NonZeroU32::new(48000).unwrap())
            .open_sink_or_fallback()
            .expect("Ошибка: Не удалось запустить аудиопоток.");

        stream.log_on_drop(false);
        let player = Player::connect_new(stream.mixer());

        Self {
            _stream: stream,
            player: Arc::new(Mutex::new(player)),
            _total_duration: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn pause(&self) {
        self.player.lock().await.pause();
    }
    pub async fn resume(&self) {
        self.player.lock().await.play();
    }

    pub async fn set_volume(&self, volume: f32) {
        self.player.lock().await.set_volume(volume.clamp(0.0, 1.0));
    }

    pub async fn get_volume(&self) -> f32 {
        self.player.lock().await.volume()
    }

    pub async fn seek_to(&self, seconds: u64) {
        let p = self.player.lock().await;
        let _ = p.try_seek(Duration::from_secs(seconds));
    }

    pub async fn get_current_pos(&self) -> u64 {
        self.player.lock().await.get_pos().as_secs()
    }

    pub async fn is_paused(&self) -> bool {
        self.player.lock().await.is_paused()
    }

    pub async fn play(&self, path: &str) -> (String, String) {
        let path_str = path.to_string();
        let player_lock = self.player.clone();
        let (artist, title, _, channels) = self.get_audio_info(&path_str).await;
    
        tokio::spawn(async move {
            let source_result = tokio::task::spawn_blocking(move || -> Option<Box<dyn Source + Send>> {
                let file = File::open(&path_str).ok()?;
                let ext = path_str.to_lowercase();
    
                // Твой Opus оставляем
                if ext.ends_with(".opus") {
                    return OpusSource::new(BufReader::new(file), channels)
                        .map(|s| Box::new(s) as Box<dyn Source + Send>);
                }
                
                Decoder::new(file).ok()
                    .map(|d| Box::new(d.track_position()) as Box<dyn Source + Send>)
            }).await;
    
            if let Ok(Some(src)) = source_result {
                let p = player_lock.lock().await;
                p.stop();   // Очищаем старые сурсы
                p.append(src);
                p.play();
            }
        });
        (artist, title)
    }
    
    pub async fn seek_relative(&self, offset_secs: i64) {
        let p = self.player.lock().await;
        
        // 1. Получаем текущую позицию
        let current_pos = p.get_pos();
        
        // 2. Считаем новую позицию максимально консервативно
        let current_secs = current_pos.as_secs_f64();
        let target_secs = (current_secs + offset_secs as f64).max(0.0);
        let target_duration = std::time::Duration::from_secs_f64(target_secs);
        
        // 3. Пытаемся мотать. Если Symphonia внутри, она ДОЛЖНА прыгнуть назад по файлу.
        if let Err(e) = p.try_seek(target_duration) {
            logger::log(&format!("AUDIO ERROR: Seek failed: {:?}", e));
        } else {
            // 4. После перемотки НАЗАД обязательно вызываем play(), 
            // чтобы плеер пересобрал очередь сэмплов с новой позиции.
            p.play();
            logger::log(&format!("AUDIO: Seek to {:.2}s successful", target_secs));
        }
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

    pub async fn is_empty(&self) -> bool {
        self.player.lock().await.empty()
    }
}
