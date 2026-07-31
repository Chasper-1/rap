use std::time::Duration;

pub enum AudioCmd {
    Play {
        path: String,
        channels: u16,
    },
    Stop,
    Pause,
    Resume,
    /// Абсолютная перемотка.
    Seek(Duration),
    /// Относительная перемотка: сдвиг от ТЕКУЩЕЙ позиции в потоке движка.
    SeekRelative(i64),
    Volume(f32),
}
