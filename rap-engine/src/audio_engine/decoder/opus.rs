use ogg::PacketReader;
use opus_decoder::OpusDecoder;
use rodio::Source;
use std::io::{Read, Seek};
use std::num::NonZero;
use std::time::Duration;

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
        let decoder = OpusDecoder::new(48000, ch).ok()?;
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
            sample_buffer: vec![0.0; OpusDecoder::MAX_FRAME_SIZE_48K * ch],
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
                let _ = self.fill();
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
        if self.packet_reader.seek_absgp(None, granule).is_ok() {
            // Сбрасываем состояние кодера, иначе остатки старой позиции
            // прорываются посторонними звуками после перемотки.
            self.decoder.reset();
            self.decoded_len = 0;
            self.buffer_pos = 0;
            self.skip_remaining = 0;
            // Сразу заполняем буфер следующим пакетом, чтобы current_span_len()
            // не вернул 0 (rodio принял бы это за конец потока).
            let _ = self.fill();
            Ok(())
        } else {
            Err(rodio::source::SeekError::NotSupported {
                underlying_source: "OpusSource",
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_frame_constant() {
        // 120 мс при 48 кГц = 5760 сэмплов на канал; буфер обязан это вмещать.
        assert!(OpusDecoder::MAX_FRAME_SIZE_48K >= 5760);
    }

    #[test]
    fn decoder_new_valid_params() {
        assert!(OpusDecoder::new(48_000, 1).is_ok());
        assert!(OpusDecoder::new(48_000, 2).is_ok());
        assert!(OpusDecoder::new(8_000, 1).is_ok());
    }

    #[test]
    fn decoder_new_invalid_params() {
        // Частота вне списка {8000, 12000, 16000, 24000, 48000} — ошибка.
        assert!(OpusDecoder::new(44_100, 2).is_err());
        // Каналов больше двух — ошибка.
        assert!(OpusDecoder::new(48_000, 3).is_err());
        // Нулевая частота — ошибка.
        assert!(OpusDecoder::new(0, 1).is_err());
    }
}
