mod audio_engine;
mod logger;
mod config;
mod parser;

use audio_engine::AudioEngine;
use std::env;
use std::io::{self, Write};
use std::time::Instant;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        logger::log("ERROR: Usage: rmpt <path>");
        return;
    }
    let path = args.join(" ");

    // 1. Проверяем, существует ли файл
    if !std::path::Path::new(&path).exists() {
        logger::log(&format!("ERROR: Файл не найден: {}", path));
        return;
    }
    logger::log(&format!("Файл существует: {}", path));

    // 2. Создаём движок
    let engine = AudioEngine::new();
    logger::log("Движок инициализирован");

    // 3. Запускаем воспроизведение и получаем метаданные
    engine.play(&path).await;

    // 4. Ждём немного, чтобы задача успела стартовать
    logger::log("Ожидание инициализации воспроизведения...");
    sleep(Duration::from_millis(200)).await;

    // 5. Проверяем, началось ли воспроизведение
    if engine.is_empty().await {
        logger::log("ERROR: Воспроизведение не началось (sink пуст)");
        logger::log("ERROR: Возможные причины:");
        logger::log("ERROR: - Неподдерживаемый формат (поддерживаются: MP3, WAV, FLAC, Ogg)");
        logger::log("ERROR: - Проблема с ALSA или звуковым устройством");
        logger::log("ERROR: - Файл повреждён");
        return;
    }

    logger::log("Воспроизведение началось");
    let start_time = Instant::now();

    // 6. Ждём окончания трека
    while !engine.is_empty().await {
        let elapsed = start_time.elapsed().as_secs();
        let minutes = elapsed / 60;
        let seconds = elapsed % 60;
        
        // Прогресс выводим в той же строке (это не лог, а интерактивный интерфейс)
        print!("\r[{:02}:{:02}] ", minutes, seconds);
        io::stdout().flush().unwrap();
        
        sleep(Duration::from_millis(200)).await;
    }

    // Финальное сообщение
    logger::log("Воспроизведение завершено");
}