use std::fs::File;
use std::num::NonZero;
use std::time::Duration;
use rodio::Source;
use symphonia::core::audio::{AudioBufferRef, Signal}; 
use symphonia::core::codecs::{Decoder as SymphoniaDecoder, DecoderOptions};
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;
use crate::logger;

pub struct SymphoniaSource {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn SymphoniaDecoder>,
    sample_buffer: Vec<f32>,
    buffer_pos: usize,
    sample_rate: u32,
    channels: u16,
    track_id: u32,
}

impl SymphoniaSource {
    pub fn new(file: File) -> Option<Self> {
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let hint = Hint::new();
        
        let probed = match symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &Default::default()) {
                Ok(p) => p,
                Err(e) => {
                    logger::log(&format!("SYMPHONIA ERROR: Failed to probe: {:?}", e));
                    return None;
                }
            };
        
        let reader = probed.format;
        let track = reader.tracks().iter()
            .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)?;
        
        let track_id = track.id;
        let codec_params = track.codec_params.clone();
        let sample_rate = codec_params.sample_rate.unwrap_or(44100);
        let channels = codec_params.channels.map(|c| c.count() as u16).unwrap_or(2);

        let decoder = match symphonia::default::get_codecs()
            .make(&codec_params, &DecoderOptions::default()) {
                Ok(d) => d,
                Err(e) => {
                    logger::log(&format!("SYMPHONIA ERROR: Decoder init failed: {:?}", e));
                    return None;
                }
            };

        logger::log(&format!("SYMPHONIA: Loaded ({}Hz, {}ch)", sample_rate, channels));

        Some(Self {
            reader,
            decoder,
            sample_buffer: Vec::with_capacity(9600), // Сразу выделим немного, чтоб не прыгало
            buffer_pos: 0,
            sample_rate,
            channels,
            track_id,
        })
    }

    fn fill_buffer(decoded: AudioBufferRef<'_>, channels: u16, out: &mut Vec<f32>) {
        out.clear(); 
        match decoded {
            AudioBufferRef::F32(buf) => {
                for i in 0..buf.frames() {
                    for ch in 0..channels as usize {
                        out.push(buf.chan(ch)[i]);
                    }
                }
            }
            AudioBufferRef::S16(buf) => {
                for i in 0..buf.frames() {
                    for ch in 0..channels as usize {
                        out.push(buf.chan(ch)[i] as f32 / 32768.0);
                    }
                }
            }
            AudioBufferRef::S32(buf) => {
                for i in 0..buf.frames() {
                    for ch in 0..channels as usize {
                        out.push(buf.chan(ch)[i] as f32 / 2147483648.0);
                    }
                }
            }
            _ => {
                logger::log("SYMPHONIA: Unsupported format during playback");
            }
        }
    }
}

impl Iterator for SymphoniaSource {
    type Item = f32;
    fn next(&mut self) -> Option<Self::Item> {
        if self.buffer_pos >= self.sample_buffer.len() {
            loop {
                let packet = match self.reader.next_packet() {
                    Ok(p) => p,
                    Err(_) => return None,
                };

                if packet.track_id() != self.track_id { continue; }
                
                match self.decoder.decode(&packet) {
                    Ok(decoded) => {
                        Self::fill_buffer(decoded, self.channels, &mut self.sample_buffer);
                        self.buffer_pos = 0;
                        if !self.sample_buffer.is_empty() { break; }
                    }
                    Err(e) => {
                        logger::log(&format!("SYMPHONIA ERROR: Decode error: {:?}", e));
                        continue;
                    }
                }
            }
        }
        let sample = self.sample_buffer.get(self.buffer_pos).cloned();
        self.buffer_pos += 1;
        sample
    }
}

impl Source for SymphoniaSource {
    fn current_span_len(&self) -> Option<usize> { None }
    fn channels(&self) -> NonZero<u16> { NonZero::new(self.channels).unwrap_or(NonZero::new(2).unwrap()) }
    fn sample_rate(&self) -> NonZero<u32> { NonZero::new(self.sample_rate).unwrap_or(NonZero::new(44100).unwrap()) }
    fn total_duration(&self) -> Option<Duration> { None }
    
    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        let seek_to = SeekTo::Time { 
            time: Time::from(pos.as_secs_f64()), 
            track_id: Some(self.track_id) 
        };
        if self.reader.seek(SeekMode::Accurate, seek_to).is_ok() {
            self.sample_buffer.clear();
            self.buffer_pos = 0;
            Ok(())
        } else {
            Err(rodio::source::SeekError::NotSupported { underlying_source: "SymphoniaSource" })
        }
    }
}