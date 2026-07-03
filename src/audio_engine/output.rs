use std::num::NonZero;

use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::stream::{OutputStream, DeviceSinkBuilder};
use rodio::Player;

use crate::logger;

pub struct AudioOutput {
    stream: OutputStream,
    player: Player,
}

impl AudioOutput {
    pub fn new() -> Result<Self, String> {
        let host = rodio::cpal::default_host();

        let device = host
            .default_output_device()
            .ok_or_else(|| "No output device found.".to_string())?;

        if let Ok(desc) = device.description() {
            logger::log(&format!("ENGINE: Using device {}", desc));
        }

        let mut stream = DeviceSinkBuilder::from_device(device)
            .map_err(|e| e.to_string())?
            .with_sample_rate(NonZero::new(48_000).unwrap())
            .open_sink_or_fallback()
            .map_err(|e| e.to_string())?;

        stream.log_on_drop(false);

        let player = Player::connect_new(stream.mixer());

        Ok(Self { stream, player })
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn player_mut(&mut self) -> &mut Player {
        &mut self.player
    }
}