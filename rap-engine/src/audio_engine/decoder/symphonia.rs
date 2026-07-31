use rodio::Source;
use std::fs::File;
use std::num::NonZero;
use std::time::Duration;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{Decoder as SymphoniaDecoder, DecoderOptions};
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

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

        let probed = match symphonia::default::get_probe().format(
            &hint,
            mss,
            &FormatOptions::default(),
            &Default::default(),
        ) {
            Ok(p) => p,
            Err(_) => {
                return None;
            }
        };

        let reader = probed.format;
        let track = reader
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)?;

        let track_id = track.id;
        let codec_params = track.codec_params.clone();
        let sample_rate = codec_params.sample_rate.unwrap_or(44100);
        let channels = codec_params.channels.map(|c| c.count() as u16).unwrap_or(2);

        let decoder = match symphonia::default::get_codecs()
            .make(&codec_params, &DecoderOptions::default())
        {
            Ok(d) => d,
            Err(_) => {
                return None;
            }
        };

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
            _ => {}
        }
    }

    /// Декодирует следующий пакет выбранного трека в буфер.
    /// Возвращает `false`, когда пакетов больше нет (конец потока или ошибка).
    fn refill(&mut self) -> bool {
        loop {
            let packet = match self.reader.next_packet() {
                Ok(p) => p,
                Err(_) => return false,
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    Self::fill_buffer(decoded, self.channels, &mut self.sample_buffer);
                    self.buffer_pos = 0;
                    if !self.sample_buffer.is_empty() {
                        return true;
                    }
                }
                Err(_) => {
                    continue;
                }
            }
        }
    }
}

impl Iterator for SymphoniaSource {
    type Item = f32;
    fn next(&mut self) -> Option<Self::Item> {
        if self.buffer_pos >= self.sample_buffer.len() && !self.refill() {
            return None;
        }
        let sample = self.sample_buffer[self.buffer_pos];
        self.buffer_pos += 1;
        // Преддекодируем следующий пакет сразу после выдачи последнего сэмпла:
        // rodio пересоздаёт конвертер частоты на границе «спана» и вызывает
        // current_span_len() именно в этот момент. Буфер должен быть заполнен,
        // чтобы остаток не оказался нулевым (Some(0) rodio считает концом потока).
        if self.buffer_pos >= self.sample_buffer.len() {
            self.refill();
        }
        Some(sample)
    }
}

impl Source for SymphoniaSource {
    fn current_span_len(&self) -> Option<usize> {
        // Остаток текущего декодированного пакета: кратен числу каналов
        // (пакеты декодируются целыми кадрами) и больше нуля, пока поток жив,
        // благодаря преддекодированию в next()/try_seek(). По этому числу rodio
        // пересоздаёт конвертер частоты на границе кадра.
        Some(self.sample_buffer.len().saturating_sub(self.buffer_pos))
    }
    fn channels(&self) -> NonZero<u16> {
        NonZero::new(self.channels).unwrap_or(NonZero::new(2).unwrap())
    }
    fn sample_rate(&self) -> NonZero<u32> {
        NonZero::new(self.sample_rate).unwrap_or(NonZero::new(44100).unwrap())
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        let seek_to = SeekTo::Time {
            time: Time::from(pos.as_secs_f64()),
            track_id: Some(self.track_id),
        };
        if self.reader.seek(SeekMode::Accurate, seek_to).is_ok() {
            // Обязательно сбрасываем внутреннее состояние декодера
            // (перекрытия кадров, предикторы и т.п.), иначе после перемотки
            // остатки старой позиции дают посторонние звуки.
            self.decoder.reset();
            self.sample_buffer.clear();
            self.buffer_pos = 0;
            // Сразу заполняем буфер следующим пакетом, чтобы current_span_len()
            // не вернул 0 (rodio принял бы это за конец потока).
            self.refill();
            Ok(())
        } else {
            Err(rodio::source::SeekError::NotSupported {
                underlying_source: "SymphoniaSource",
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;
    use symphonia::core::audio::{AudioBuffer, Channels, SignalSpec};

    const STEREO: Channels = Channels::FRONT_LEFT.union(Channels::FRONT_RIGHT);

    fn render(buf: &mut AudioBuffer<f32>, frames: usize) {
        buf.render_reserved(Some(frames));
        let mut v = 1.0f32;
        for ch in 0..2 {
            for s in buf.chan_mut(ch) {
                *s = v;
                v += 1.0;
            }
        }
    }

    #[test]
    fn fill_buffer_interleaves_stereo() {
        let mut buf = AudioBuffer::<f32>::new(3, SignalSpec::new(48000, STEREO));
        render(&mut buf, 3);

        let mut out = Vec::new();
        SymphoniaSource::fill_buffer(AudioBufferRef::F32(Cow::Borrowed(&buf)), 2, &mut out);

        // Поканальные сэмплы должны переплестись в порядок L,R,L,R,L,R:
        // канал 0 = 1,2,3; канал 1 = 4,5,6.
        assert_eq!(out, vec![1.0, 4.0, 2.0, 5.0, 3.0, 6.0]);
    }

    #[test]
    fn fill_buffer_mono_uses_first_channel_only() {
        let mut buf = AudioBuffer::<f32>::new(3, SignalSpec::new(48000, STEREO));
        render(&mut buf, 3);

        let mut out = Vec::new();
        SymphoniaSource::fill_buffer(AudioBufferRef::F32(Cow::Borrowed(&buf)), 1, &mut out);

        assert_eq!(out, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn fill_buffer_scales_s16() {
        let mut buf = AudioBuffer::<i16>::new(2, SignalSpec::new(48000, Channels::FRONT_CENTRE));
        buf.render_reserved(Some(2));
        buf.chan_mut(0).copy_from_slice(&[32767, -32768]);

        let mut out = Vec::new();
        SymphoniaSource::fill_buffer(AudioBufferRef::S16(Cow::Borrowed(&buf)), 1, &mut out);

        assert_eq!(out.len(), 2);
        assert!((out[0] - 32767.0 / 32768.0).abs() < f32::EPSILON);
        assert_eq!(out[1], -1.0);
    }
}
