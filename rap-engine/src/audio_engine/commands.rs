use std::time::Duration;

pub enum AudioCmd {
    Play {
        path: String,
        channels: u16,
    },
    // Запуск трека СРАЗУ в паузе (источник добавляется уже в паузе,
    // звук не успевает зазвучать) + перемотка к позиции.
    PlayPaused {
        path: String,
        channels: u16,
        seek_secs: u64,
    },
    Stop,
    Pause,
    Resume,
    // Абсолютная перемотка.
    Seek(Duration),
    // Относительная перемотка: сдвиг от ТЕКУЩЕЙ позиции в потоке движка.
    SeekRelative(i64),
    Volume(f32),
}
