use rap_engine::AudioEngine;
use std::env;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Путь к аудиофайлу передаётся аргументом: rap <путь/к/файлу>
    let args: Vec<String> = env::args().skip(1).collect();
    let path = args.join(" ");

    if path.is_empty() {
        eprintln!("Usage: rap <path-to-audio-file>");
        return Ok(());
    }

    if !std::path::Path::new(&path).exists() {
        eprintln!("File not found: {}", path);
        return Ok(());
    }

    let mut engine = AudioEngine::new(None);

    engine.play(&path).await;

    // Ждём, пока трек начнёт играть (загрузится и не будет пустым)
    let mut loaded = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if !engine.is_empty() {
            loaded = true;
            break;
        }
    }

    if !loaded {
        eprintln!("Failed to load track: {}", path);
        engine.shutdown().await;
        return Ok(());
    }

    // Ждём конца воспроизведения
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if engine.is_empty() {
            break;
        }
    }

    engine.shutdown().await;
    Ok(())
}
