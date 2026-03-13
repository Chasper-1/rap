use chrono::Local;
use std::sync::Mutex;
use std::path::{PathBuf};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;
use std::io::Write; // Для синхронного flush
use crate::config::config::Config;

static LOG_CACHE: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn get_cache_dir() -> PathBuf {
    let mut path = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("rmpt");
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

/// Асинхронный сброс для штатного выхода (использует Tokio)
pub async fn final_flush() {
    let conf = Config::global();
    let max_logs = conf.logging.max_logs.max(1);
    let cache_dir = get_cache_dir();
    
    rotate_logs(&cache_dir, max_logs).await;

    let lines_to_write = {
        let Ok(mut cache) = LOG_CACHE.lock() else { return };
        if cache.is_empty() { return; }
        std::mem::take(&mut *cache)
    };

    // Печатаем в stderr для наглядности при выходе
    eprintln!("\n--- RMPT LOGS ---");
    for line in &lines_to_write {
        eprintln!("{}", line);
    }
    eprintln!("-----------------\n");

    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S");
    let mut log_path = cache_dir;
    log_path.push(format!("rmpt_{}.log", timestamp));

    if let Ok(mut file) = File::create(log_path).await {
        for line in lines_to_write {
            let _ = file.write_all(format!("{}\n", line).as_bytes()).await;
        }
        let _ = file.flush().await;
    }
}

/// СИНХРОННЫЙ экстренный сброс для паник-хендлера
/// Не требует асинхронного рантайма и не блокирует его
pub fn emergency_flush(panic_info: &std::panic::PanicHookInfo) {
    let Ok(mut cache) = LOG_CACHE.try_lock() else {
        eprintln!("Logger error: Could not lock cache during panic.");
        return;
    };

    if cache.is_empty() && panic_info.to_string().is_empty() { return; }

    let cache_dir = get_cache_dir();
    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S");
    let log_path = cache_dir.join(format!("panic_{}.log", timestamp));

    // Используем std::fs::File (синхронный)
    if let Ok(mut file) = std::fs::File::create(log_path) {
        let _ = writeln!(file, "!!! RMPT CRITICAL PANIC !!!");
        let _ = writeln!(file, "Reason: {}", panic_info);
        let _ = writeln!(file, "--- Cached Logs ---");
        for line in cache.iter() {
            let _ = writeln!(file, "{}", line);
        }
        let _ = file.flush();
        eprintln!("Emergency logs saved to: {:?}", get_cache_dir());
    }
    
    // Очищаем кэш, чтобы не дублировать при повторном вызове
    cache.clear();
}

async fn rotate_logs(path: &PathBuf, max: usize) {
    let mut entries = match fs::read_dir(path).await {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut logs = Vec::new();
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