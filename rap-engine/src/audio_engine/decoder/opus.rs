use ogg::PacketReader;
use opus_codec::{Channels, Decoder as OpusDecoder, SampleRate as OpusSampleRate};
use rodio::Source;
use std::io::{Read, Seek};
use std::num::NonZero;
use std::time::Duration;

pub struct OpusSource<R: Read + Seek> {
    packet_reader: PacketReader<R>,
    decoder: OpusDecoder,
    sample_buffer: Vec<f32>, // Переиспользуем этот буфер
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

                        // Ресайзим существующий буфер вместо создания нового vec!
                        let target_size = 5760 * self.channels as usize;
                        if self.sample_buffer.len() < target_size {
                            self.sample_buffer.resize(target_size, 0.0);
                        }

                        if let Ok(decoded_size) =
                            self.decoder
                                .decode_float(&packet.data, &mut self.sample_buffer, false)
                        {
                            // Отрезаем лишнее в конце, если декодировалось меньше (без выделения памяти)
                            self.sample_buffer
                                .truncate(decoded_size * self.channels as usize);
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
