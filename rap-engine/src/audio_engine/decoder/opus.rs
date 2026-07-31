use ogg::PacketReader;
use opus_codec::{Channels, Decoder as OpusDecoder, SampleRate as OpusSampleRate};
use rodio::Source;
use std::io::{Read, Seek};
use std::num::NonZero;
use std::time::Duration;

/// Максимальный кадр Opus при 48 кГц: 120 мс * 48000 = 5760 сэмплов на канал.
const MAX_FRAME_SAMPLES: usize = 5760;

pub struct OpusSource<R: Read + Seek> {
    packet_reader: PacketReader<R>,
    decoder: OpusDecoder,
    /// Буфер переиспользуется между кадрами (выделяется один раз).
    sample_buffer: Vec<f32>,
    /// Длина декодированных сэмплов в буфере (на все каналы).
    decoded_len: usize,
    buffer_pos: usize,
    /// Pre-skip из OpusHead: пропускается в начале (и после seek сбрасывается).
    skip_remaining: usize,
    sample_rate: u32,
    channels: u16,
}

impl<R: Read + Seek> OpusSource<R> {
    pub fn new(reader: R, channels: u16) -> Option<Self> {
        let ch = usize::from(channels).clamp(1, 2);
        let opus_channels = if ch == 1 {
            Channels::Mono
        } else {
            Channels::Stereo
        };
        let decoder = OpusDecoder::new(OpusSampleRate::Hz48000, opus_channels).ok()?;
        let mut packet_reader = PacketReader::new(reader);

        // Pre-skip из первого пакета OpusHead (байты 10..12, little-endian):
        // столько сэмплов кодек «прогревает» тишиной — их нужно пропустить.
        let skip_remaining = match packet_reader.read_packet() {
            Ok(Some(p)) if p.data.starts_with(b"OpusHead") && p.data.len() >= 12 => {
                u16::from_le_bytes([p.data[10], p.data[11]]) as usize * ch
            }
            _ => 0,
        };

        Some(Self {
            packet_reader,
            decoder,
            sample_buffer: vec![0.0; MAX_FRAME_SAMPLES * ch],
            decoded_len: 0,
            buffer_pos: 0,
            skip_remaining,
            sample_rate: 48000,
            channels: ch as u16,
        })
    }

    /// Декодирует следующий аудиопакет в буфер. Возвращает `false` на конце потока.
    fn fill(&mut self) -> bool {
        loop {
            match self.packet_reader.read_packet() {
                Ok(Some(packet)) => {
                    if packet.data.starts_with(b"OpusHead") || packet.data.starts_with(b"OpusTags")
                    {
                        continue;
                    }
                    if let Ok(decoded) =
                        self.decoder
                            .decode_float(&packet.data, &mut self.sample_buffer, false)
                    {
                        self.decoded_len = decoded * self.channels as usize;
                        self.buffer_pos = 0;
                        return true;
                    }
                }
                _ => return false,
            }
        }
    }
}

impl<R: Read + Seek> Iterator for OpusSource<R> {
    type Item = f32;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Пропускаем pre-skip сэмплы, не выдавая их наружу.
            if self.skip_remaining > 0 {
                if self.buffer_pos >= self.decoded_len && !self.fill() {
                    return None;
                }
                self.buffer_pos += 1;
                self.skip_remaining -= 1;
                continue;
            }
            if self.buffer_pos >= self.decoded_len && !self.fill() {
                return None;
            }
            let sample = self.sample_buffer[self.buffer_pos];
            self.buffer_pos += 1;
            // Преддекодируем следующий пакет сразу после выдачи последнего сэмпла:
            // rodio пересоздаёт конвертер частоты на границе «спана» и вызывает
            // current_span_len() именно в этот момент. Буфер должен быть заполнен,
            // чтобы остаток не оказался нулевым (Some(0) rodio считает концом потока).
            if self.buffer_pos >= self.decoded_len {
                self.fill();
            }
            return Some(sample);
        }
    }
}

impl<R: Read + Seek + Send> Source for OpusSource<R> {
    fn current_span_len(&self) -> Option<usize> {
        // Остаток текущего декодированного пакета: кратен числу каналов
        // (пакеты декодируются целыми кадрами) и больше нуля, пока поток жив,
        // благодаря преддекодированию в next()/try_seek(). По этому числу rodio
        // пересоздаёт конвертер частоты на границе кадра.
        Some(self.decoded_len.saturating_sub(self.buffer_pos))
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
        if self.packet_reader.seek_absgp(None, granule).is_err() {
            return Err(rodio::source::SeekError::NotSupported {
                underlying_source: "OpusSource",
            });
        }
        // Сбрасываем состояние кодера, иначе остатки старой позиции
        // прорываются посторонними звуками после перемотки.
        self.decoder
            .reset()
            .map_err(|_| rodio::source::SeekError::NotSupported {
                underlying_source: "OpusSource",
            })?;
        self.decoded_len = 0;
        self.buffer_pos = 0;
        self.skip_remaining = 0;
        // Сразу заполняем буфер следующим пакетом, чтобы current_span_len()
        // не вернул 0 (rodio принял бы это за конец потока).
        self.fill();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_new_valid_params() {
        assert!(OpusDecoder::new(OpusSampleRate::Hz48000, Channels::Mono).is_ok());
        assert!(OpusDecoder::new(OpusSampleRate::Hz48000, Channels::Stereo).is_ok());
        assert!(OpusDecoder::new(OpusSampleRate::Hz8000, Channels::Mono).is_ok());
    }

    #[test]
    fn channels_as_usize() {
        assert_eq!(Channels::Mono.as_usize(), 1);
        assert_eq!(Channels::Stereo.as_usize(), 2);
    }

    #[test]
    fn sample_rate_is_valid() {
        assert!(OpusSampleRate::Hz48000.is_valid());
        assert!(OpusSampleRate::Hz8000.is_valid());
        assert!(OpusSampleRate::Hz24000.is_valid());
    }

    #[test]
    fn decode_float_rejects_small_output() {
        let mut decoder = OpusDecoder::new(OpusSampleRate::Hz48000, Channels::Stereo).unwrap();
        // Пустой буфер и буфер, некратный каналам, отклоняются до декодирования.
        assert!(decoder.decode_float(&[], &mut [], false).is_err());
        assert!(
            decoder
                .decode_float(&[0xFF, 0xFE], &mut [0.0f32; 3], false)
                .is_err()
        );
    }

    #[test]
    fn decode_float_rejects_oversized_buffer() {
        let mut decoder = OpusDecoder::new(OpusSampleRate::Hz48000, Channels::Stereo).unwrap();
        // Буфер больше максимального кадра (5760 сэмплов на канал) отклоняется.
        let mut big = vec![0.0f32; (MAX_FRAME_SAMPLES + 1) * 2];
        assert!(
            decoder
                .decode_float(&[0xFF, 0xFE], &mut big, false)
                .is_err()
        );
    }
}
