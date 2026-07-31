use std::time::Duration;

pub enum AudioCmd {
    Play { path: String, channels: u16 },
    Stop,
    Pause,
    Resume,
    Seek(Duration),
    Volume(f32),
}