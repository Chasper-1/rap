use rodio::Source;
use std::fs::File;
use std::num::NonZero;
use std::time::Duration;
use symphonia::core::audio::GenericAudioBufferRef;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions, CODEC_ID_NULL_AUDIO};
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::units::Time;

pub struct SymphoniaSource {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
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

        let reader = match symphonia::default::get_probe().probe(
            &hint,
            mss,
            FormatOptions::default(),
            Default::default(),
        ) {
            Ok(r) => r,
            Err(_) => {
                return None;
            }
        };

        let track = reader.tracks().iter().find(|t| match &t.codec_params {
            Some(CodecParameters::Audio(p)) => p.codec != CODEC_ID_NULL_AUDIO,
            _ => false,
        })?;

        let track_id = track.id;
        let params = match &track.codec_params {
            Some(CodecParameters::Audio(p)) => p.clone(),
            _ => return None,
        };
        let sample_rate = params.sample_rate.unwrap_or(44100);
        let channels = params
            .channels
            .as_ref()
            .map(|c| c.count() as u16)
            .unwrap_or(2);

        let decoder = match symphonia::default::get_codecs()
            .make_audio_decoder(&params, &AudioDecoderOptions::default())
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

    // Конвертирует декодированный буфер любого формата в interleaved f32.
    fn fill_buffer(decoded: GenericAudioBufferRef<'_>, out: &mut Vec<f32>) {
        out.clear();
        decoded.copy_to_vec_interleaved::<f32>(out);
    }

    // Декодирует следующий пакет выбранного трека в буфер.
    // Возвращает `false`, когда пакетов больше нет (конец потока или ошибка).
    fn refill(&mut self) -> bool {
        loop {
            let packet = match self.reader.next_packet() {
                Ok(Some(p)) => p,
                _ => return false,
            };

            if packet.track_id != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    Self::fill_buffer(decoded, &mut self.sample_buffer);
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
        let Some(time) = Time::try_from_secs_f64(pos.as_secs_f64()) else {
            return Err(rodio::source::SeekError::NotSupported {
                underlying_source: "SymphoniaSource",
            });
        };
        let seek_to = SeekTo::Time {
            time,
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
    use symphonia::core::audio::{AudioBuffer, AudioMut, AudioSpec, Channels};

    const STEREO: Channels = symphonia::core::audio::layouts::CHANNEL_LAYOUT_STEREO;

    fn render(buf: &mut AudioBuffer<f32>, frames: usize) {
        buf.render_uninit(Some(frames));
        let mut v = 1.0f32;
        for ch in 0..2 {
            for s in buf.plane_mut(ch).unwrap() {
                *s = v;
                v += 1.0;
            }
        }
    }

    #[test]
    fn fill_buffer_interleaves_stereo() {
        let mut buf = AudioBuffer::<f32>::new(AudioSpec::new(48000, STEREO), 3);
        render(&mut buf, 3);

        let mut out = Vec::new();
        SymphoniaSource::fill_buffer(GenericAudioBufferRef::F32(&buf), &mut out);

        // Поканальные сэмплы должны переплестись в порядок L,R,L,R,L,R:
        // канал 0 = 1,2,3; канал 1 = 4,5,6.
        assert_eq!(out, vec![1.0, 4.0, 2.0, 5.0, 3.0, 6.0]);
    }

    #[test]
    fn fill_buffer_mono_passthrough() {
        // Моно-буфер интерливится как есть: канал 0 = 1,2,3.
        let mut buf = AudioBuffer::<f32>::new(
            AudioSpec::new(48000, symphonia::core::audio::layouts::CHANNEL_LAYOUT_MONO),
            3,
        );
        buf.render_uninit(Some(3));
        buf.plane_mut(0).unwrap().copy_from_slice(&[1.0, 2.0, 3.0]);

        let mut out = Vec::new();
        SymphoniaSource::fill_buffer(GenericAudioBufferRef::F32(&buf), &mut out);

        assert_eq!(out, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn fill_buffer_scales_s16() {
        let mut buf = AudioBuffer::<i16>::new(
            AudioSpec::new(
                48000,
                Channels::Positioned(symphonia::core::audio::Position::FRONT_CENTER),
            ),
            2,
        );
        buf.render_uninit(Some(2));
        buf.plane_mut(0).unwrap().copy_from_slice(&[32767, -32768]);

        let mut out = Vec::new();
        SymphoniaSource::fill_buffer(GenericAudioBufferRef::S16(&buf), &mut out);

        assert_eq!(out.len(), 2);
        assert!((out[0] - 32767.0 / 32768.0).abs() < f32::EPSILON);
        assert_eq!(out[1], -1.0);
    }
}
