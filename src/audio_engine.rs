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

        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            let stream = device.build_output_stream(
                &config.into(),
                move |data: &mut [f32], _| {
                    for sample in data.iter_mut() {
                        if buffer.is_empty() {
                            if let Ok(new_frame) = rx.try_recv() {
                                buffer = new_frame;
                            }
                        }
                        *sample = if !buffer.is_empty() { buffer.remove(0) } else { 0.0 };
                    }
                },
                |err| logger::log(&format!("Error: CPAL stream - {}", err)),
                None
            ).unwrap();
            stream.play().unwrap();
            loop { std::thread::sleep(std::time::Duration::from_millis(10)); }
        });

        Self { tx }
    }

    pub async fn play(&self, path: &str) -> (String, String) {
        let tx = self.tx.clone();
        let path_str = path.to_string();
        
        let mut artist = "Unknown".to_string();
        let mut title = "Unknown".to_string();

        if let Ok(ictx) = ffmpeg::format::input(&Path::new(&path_str)) {
            for (key, value) in ictx.metadata().iter() {
                match key.to_lowercase().as_str() {
                    "artist" => artist = value.to_string(),
                    "title" => title = value.to_string(),
                    _ => {}
                }
            }
        }

        logger::log(&format!("Engine: Decoding started [{}]", path_str));

        tokio::task::spawn_blocking(move || {
            let mut ictx = ffmpeg::format::input(&Path::new(&path_str)).unwrap();
            let stream = ictx.streams().best(ffmpeg::media::Type::Audio).unwrap();
            let stream_index = stream.index();
            let context = ffmpeg::codec::context::Context::from_parameters(stream.parameters()).unwrap();
            let mut decoder = context.decoder().audio().unwrap();

            let mut resampler = ffmpeg::software::resampling::context::Context::get(
                decoder.format(),
                decoder.channel_layout(),
                decoder.rate(),
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
            logger::log("Engine: Track finished");
        });

        (artist, title)
    }

    pub fn is_empty(&self) -> bool { false }
}