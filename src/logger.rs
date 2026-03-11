use chrono::Local;
use std::sync::Mutex;
use std::path::PathBuf;
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt; // Для writeln! и write_all
use crate::config::config::Config;

static LOG_CACHE: Mutex<Vec<String>> = Mutex::new(Vec::new());

// Тут всё ок, это просто работа с путями
fn get_cache_dir() -> PathBuf {
    let mut path = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("rmpt");
    // Оставляем синхронно или делаем проверку при старте, 
    // но для простоты при вызове log пускай будет так
    if !path.exists() {
        let _ = std::fs::create_dir_all(&path);
    }
    path
}

pub fn log(message: &str) {
    let timestamp = Local::now().format("%H:%M:%S");
    if let Ok(mut cache) = LOG_CACHE.lock() {
        cache.push(format!("[{}] {}", timestamp, message));
    }
}

// ТЕПЕРЬ ЭТО ASYNC
pub async fn final_flush() {
    let conf = Config::global();
    let max_logs = conf.logging.max_logs.max(1);
    
    let cache_dir = get_cache_dir();
    
    // Ротацию тоже делаем асинхронной
    rotate_logs(&cache_dir, max_logs).await;

    // Вынимаем логи из кэша
    let lines_to_write = {
        let mut cache = LOG_CACHE.lock().unwrap();
        if cache.is_empty() { return; }
        std::mem::take(&mut *cache) // Забираем всё и очищаем кэш разом
    };

    // 1. Вывод в консоль (stderr)
    eprintln!("\n--- RMPT LOGS ---");
    for line in &lines_to_write {
        eprintln!("{}", line);
    }
    eprintln!("-----------------\n");

    // 2. Асинхронная запись в файл
    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S");
    let mut log_path = cache_dir.clone();
    log_path.push(format!("rmpt_{}.log", timestamp));

    match File::create(log_path).await {
        Ok(mut file) => {
            for line in lines_to_write {
                let _ = file.write_all(format!("{}\n", line).as_bytes()).await;
            }
            let _ = file.flush().await;
        }
        Err(e) => eprintln!("Failed to create log file: {}", e),
    }
}

async fn rotate_logs(path: &PathBuf, max: usize) {
    let mut entries = match fs::read_dir(path).await {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut logs = Vec::new();

    // Асинхронно перебираем файлы
    while let Ok(Some(entry)) = entries.next_entry().await {
        if let Ok(name) = entry.file_name().into_string() {
            if name.starts_with("rmpt_") && name.ends_with(".log") {
                logs.push(entry.path());
            }
        }
    }

    if logs.len() >= max {
        logs.sort();
        let to_remove = logs.len().saturating_sub(max - 1);
        for i in 0..to_remove {
            let _ = fs::remove_file(&logs[i]).await;
        }
    }
}