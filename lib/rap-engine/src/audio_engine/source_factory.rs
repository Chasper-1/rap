use std::fs::File;
use std::io::BufReader;

use rodio::Source;

use crate::audio_engine::decoder::{OpusSource, SymphoniaSource};

pub async fn open_source(
    path: &str,
    channels: u16,
) -> Option<Box<dyn Source + Send>> {
    let path = path.to_owned();

    tokio::task::spawn_blocking(move || {
        let file = File::open(&path).ok()?;

        match path.rsplit('.').next() {
            Some(ext) if ext.eq_ignore_ascii_case("opus") => {
                OpusSource::new(BufReader::new(file), channels)
                    .map(|src| Box::new(src) as Box<dyn Source + Send>)
            }

            _ => {
                SymphoniaSource::new(file)
                    .map(|src| Box::new(src) as Box<dyn Source + Send>)
            }
        }
    })
    .await
    .ok()?
}