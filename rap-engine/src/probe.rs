use std::fs::File;
use std::path::Path;
use std::time::Duration;

use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::probe::Hint;

/// Определяет длительность аудиофайла из его заголовков и метаданных,
/// без полного декодирования.
///
/// Возвращает `None`, если формат не распознан или длительность
/// не указана в метаданных (например, потоковый формат).
pub fn probe_duration(path: &Path) -> Option<Duration> {
    let file = File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let probed = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            mss,
            &FormatOptions::default(),
            &Default::default(),
        )
        .ok()?;

    let track = probed
        .format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)?;

    let n_frames = track.codec_params.n_frames?;
    let sample_rate = track.codec_params.sample_rate?;

    Some(Duration::from_secs_f64(
        n_frames as f64 / sample_rate as f64,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Генерирует простой WAV-файл (PCM 16-bit mono) заданной длительности.
    fn write_wav(path: &Path, sample_rate: u32, seconds: u32) {
        let n_samples = sample_rate * seconds;
        let data_len = n_samples * 2;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
        wav.extend_from_slice(&2u16.to_le_bytes()); // block align
        wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.resize(wav.len() + data_len as usize, 0);

        let mut f = File::create(path).unwrap();
        f.write_all(&wav).unwrap();
    }

    #[test]
    fn probe_wav_duration() {
        let dir = std::env::temp_dir();
        let path = dir.join("rap_probe_test_1s.wav");
        write_wav(&path, 44100, 1);
        let dur = probe_duration(&path).expect("длительность должна определиться");
        assert!((dur.as_secs_f64() - 1.0).abs() < 0.05);
        std::fs::remove_file(&path).expect("тестовый файл не удалился");
    }

    #[test]
    fn probe_wav_duration_10s() {
        let dir = std::env::temp_dir();
        let path = dir.join("rap_probe_test_10s.wav");
        write_wav(&path, 22050, 10);
        let dur = probe_duration(&path).expect("длительность должна определиться");
        assert!((dur.as_secs_f64() - 10.0).abs() < 0.05);
        std::fs::remove_file(&path).expect("тестовый файл не удалился");
    }

    #[test]
    fn probe_missing_file() {
        assert_eq!(
            probe_duration(Path::new("/nonexistent/definitely_missing.mp3")),
            None
        );
    }

    #[test]
    fn probe_not_audio() {
        let dir = std::env::temp_dir();
        let path = dir.join("rap_probe_test.txt");
        std::fs::write(&path, b"hello").unwrap();
        assert_eq!(probe_duration(&path), None);
        std::fs::remove_file(&path).expect("тестовый файл не удалился");
    }
}
