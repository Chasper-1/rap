use ogg::PacketReader;
use rodio::Source;
use rusty_opus::OpusDecoder;
use std::io::{Read, Seek};
use std::num::NonZero;
use std::time::Duration;

// Максимальный кадр Opus при 48 кГц: 120 мс * 48000 = 5760 сэмплов на канал.
const MAX_FRAME_SAMPLES: usize = 5760;

// Сэмплов на канал для одного кадра при 48 кГц по TOC-конфигу (как в
// examples/decode_bit.rs из rusty-opus: 2.5 мс = 120 сэмплов).
fn samples_per_frame_48k(config: u8) -> usize {
    match config {
        0..=11 => {
            // SILK: 10/20/40/60 мс.
            let ms = [10usize, 20, 40, 60][(config % 4) as usize];
            ms * 48
        }
        12..=15 => {
            // Hybrid: 10/20 мс.
            let ms = [10usize, 20][(config % 2) as usize];
            ms * 48
        }
        _ => {
            // CELT: 2.5/5/10/20 мс.
            match config % 4 {
                0 => 120,
                1 => 240,
                2 => 480,
                _ => 960,
            }
        }
    }
}

// Суммарный размер кадра пакета (сэмплов на канал) по его TOC-байту.
fn packet_frame_size(payload: &[u8], rate: i32) -> usize {
    let Some(&toc) = payload.first() else {
        return 960; // Пустой пакет = PLC: декодер выдаст кадр тишины.
    };
    let config = toc >> 3;
    let code = toc & 0x03;
    let frames = match code {
        0 => 1,
        1 | 2 => 2,
        _ => {
            if payload.len() >= 2 {
                usize::from(payload[1] & 0x3F)
            } else {
                1
            }
        }
    };
    let per = samples_per_frame_48k(config) * rate as usize / 48000;
    per * frames
}

pub struct OpusSource<R: Read + Seek> {
    packet_reader: PacketReader<R>,
    decoder: OpusDecoder,
    // Буфер переиспользуется между кадрами (выделяется один раз).
    sample_buffer: Vec<f32>,
    // Длина декодированных сэмплов в буфере (на все каналы).
    decoded_len: usize,
    buffer_pos: usize,
    // Pre-skip из OpusHead: пропускается в начале (и после seek сбрасывается).
    skip_remaining: usize,
    sample_rate: u32,
    channels: u16,
}

impl<R: Read + Seek> OpusSource<R> {
    pub fn new(reader: R, channels: u16) -> Option<Self> {
        let ch = usize::from(channels).clamp(1, 2);
        // rusty-opus — чистый Rust: SIMD-ядра (SSE2/AVX/AVX2/FMA, NEON на ARM)
        // подключаются runtime-детекцией при декодировании, без флагов сборки.
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
            sample_buffer: vec![0.0; MAX_FRAME_SAMPLES * ch],
            decoded_len: 0,
            buffer_pos: 0,
            skip_remaining,
            sample_rate: 48000,
            channels: ch as u16,
        })
    }

    // Декодирует следующий аудиопакет в буфер. Возвращает `false` на конце потока.
    fn fill(&mut self) -> bool {
        loop {
            match self.packet_reader.read_packet() {
                Ok(Some(packet)) => {
                    if packet.data.starts_with(b"OpusHead") || packet.data.starts_with(b"OpusTags")
                    {
                        continue;
                    }
                    // frame_size для decode() должен быть ТОЧНЫМ размером кадра из
                    // TOC пакета: декодер возвращает переданный frame_size, а в
                    // буфер пишет только фактический (count * n * каналов).
                    let fs = packet_frame_size(&packet.data, 48000).min(MAX_FRAME_SAMPLES);
                    if let Ok(decoded) =
                        self.decoder
                            .decode(&packet.data, fs, &mut self.sample_buffer)
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
        // Сбрасываем состояние декодера, иначе остатки старой позиции
        // прорываются посторонними звуками после перемотки (API rusty-opus
        // не имеет reset() — декодер пересоздаётся, это дёшево).
        self.decoder = OpusDecoder::new(48000, self.channels as usize).map_err(|_| {
            rodio::source::SeekError::NotSupported {
                underlying_source: "OpusSource",
            }
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
        assert!(OpusDecoder::new(48000, 1).is_ok());
        assert!(OpusDecoder::new(48000, 2).is_ok());
        assert!(OpusDecoder::new(8000, 1).is_ok());
    }

    #[test]
    fn decode_empty_packet_is_plc() {
        // Пустой пакет = потеря пакета: декодер выдаёт concealment
        // (кадр тишины/экстраполяции), а не ошибку.
        let mut decoder = OpusDecoder::new(48000, 2).unwrap();
        let mut buf = vec![0.0f32; 960 * 2];
        assert_eq!(decoder.decode(&[], 960, &mut buf).unwrap(), 960);
    }

    #[test]
    fn decode_rejects_small_output() {
        let mut decoder = OpusDecoder::new(48000, 2).unwrap();
        // Буфер меньше кадра (frame_size * каналы) декодировать нельзя.
        assert!(
            decoder
                .decode(&[0xFF, 0xFE], 5760, &mut [0.0f32; 3])
                .is_err()
        );
    }

    #[test]
    fn frame_size_matches_toc_coding() {
        // 0xF8: CELT, fullband, 20 мс, mono -> 960 сэмплов/канал (48 кГц).
        assert_eq!(packet_frame_size(&[0xF8], 48000), 960);
        // 0x00: SILK 10 мс, 1 кадр -> 480 сэмплов/канал.
        assert_eq!(packet_frame_size(&[0x00], 48000), 480);
        // 0x41: SILK 10 мс (config 8), 2 кадра (code=1) -> 2 * 480.
        assert_eq!(packet_frame_size(&[0x41], 48000), 960);
    }

    #[test]
    fn roundtrip_encode_decode_produces_audio() {
        use rusty_opus::{Application, OpusEncoder};
        // Реальный цикл: кодируем синус 20 мс, декодируем с точным размером
        // кадра из TOC. Сэмплы должны быть слышимыми (не нулями) — это ловит
        // регрессию, когда decode() получал константу 5760 вместо размера кадра.
        let mut encoder = OpusEncoder::new(48000, 1, Application::Audio).unwrap();
        let mut decoder = OpusDecoder::new(48000, 1).unwrap();
        let frame_size = 960usize;
        let input: Vec<f32> = (0..frame_size)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48000.0).sin())
            .collect();
        let mut packet = [0u8; 4096];
        let len = encoder.encode(&input, frame_size, &mut packet).unwrap();
        let pkt = &packet[..len];
        assert_eq!(packet_frame_size(pkt, 48000), frame_size);
        let mut out = vec![0.0f32; frame_size];
        assert_eq!(
            decoder.decode(pkt, frame_size, &mut out).unwrap(),
            frame_size
        );
        let peak = out.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
        assert!(peak > 0.1, "декодер выдал тишину: peak={peak}");
    }
}
