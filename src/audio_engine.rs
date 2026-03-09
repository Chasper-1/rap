use ffmpeg_next as ffmpeg;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tokio::sync::mpsc;
use std::path::Path;
use crate::logger;

pub struct AudioEngine {
    tx: mpsc::UnboundedSender<Vec<f32>>,
}

impl AudioEngine {
    pub fn new() -> Self {
        logger::log("System: Init FFmpeg...");
        ffmpeg::init().expect("FFmpeg init failed");

        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<f32>>();
        let host = cpal::default_host();
        let device = host.default_output_device().expect("No audio device");
        let config = device.default_output_config().unwrap();

        // CPAL просто выгребает всё, что прилетело в канал
        std::thread::spawn(move || {
            let stream = device.build_output_stream(
                &config.into(),
                move |data: &mut [f32], _| {
                    if let Ok(frame) = rx.try_recv() {
                        let len = frame.len().min(data.len());
                        data[..len].copy_from_slice(&frame[..len]);
                    }
                },
                |err| logger::log(&format!("Error: CPAL stream - {}", err)),
                None
            ).unwrap();
            stream.play().unwrap();
            loop { std::thread::sleep(std::time::Duration::from_millis(5)); }
        });

        Self { tx }
    }

    pub async fn play(&self, path: &str) -> (String, String) {
        let tx = self.tx.clone();
        let path_str = path.to_string();
        
        // Метаданные вытаскиваем один раз через FFmpeg
        let (artist, title) = if let Ok(ictx) = ffmpeg::format::input(&Path::new(&path_str)) {
            let mut a = "Unknown".to_string();
            let mut t = "Unknown".to_string();
            for (key, value) in ictx.metadata().iter() {
                match key.to_lowercase().as_str() {
                    "artist" => a = value.to_string(),
                    "title" => t = value.to_string(),
                    _ => {}
                }
            }
            (a, t)
        } else {
            ("Unknown".to_string(), "Unknown".to_string())
        };

        logger::log(&format!("Engine: Playing {} - {}", artist, title));

        tokio::task::spawn_blocking(move || {
            let mut ictx = ffmpeg::format::input(&Path::new(&path_str)).unwrap();
            let stream = ictx.streams().best(ffmpeg::media::Type::Audio).unwrap();
            let stream_index = stream.index();
            let mut decoder = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
                .unwrap()
                .decoder()
                .audio()
                .unwrap();

            // Пусть FFmpeg сам ресемплит Opus (48k) в то, что хочет твоя система
            let mut resampler = decoder.resampler(
                ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed),
                ffmpeg::ChannelLayout::STEREO,
                48000,
            ).unwrap();

            for (stream, packet) in ictx.packets() {
                if stream.index() == stream_index {
                    if decoder.send_packet(&packet).is_ok() {
                        let mut decoded = ffmpeg::frame::Audio::empty();
                        while decoder.receive_frame(&mut decoded).is_ok() {
                            let mut resampled = ffmpeg::frame::Audio::empty();
                            resampler.run(&decoded, &mut resampled).unwrap();
                            let _ = tx.send(resampled.plane(0).to_vec());
                        }
                    }
                }
            }
        });

        (artist, title)
    }

    pub fn is_empty(&self) -> bool { false }
}