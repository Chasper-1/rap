use chrono::Local;
use std::fs::{self, File};
use std::io::Write;
use std::sync::Mutex;
use std::path::PathBuf;
use crate::config::config::Config;

static LOG_CACHE: Mutex<Vec<String>> = Mutex::new(Vec::new());

// Функция для получения пути к папке кэша
fn get_cache_dir() -> PathBuf {
    let mut path = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("rmpt");
    if !path.exists() {
        let _ = fs::create_dir_all(&path);
    }
    path
}

pub fn log(message: &str) {
    let timestamp = Local::now().format("%H:%M:%S");
    if let Ok(mut cache) = LOG_CACHE.lock() {
        cache.push(format!("[{}] {}", timestamp, message));
    }
}

pub fn final_flush() {
    let conf = Config::global();
    let max_logs = conf.logging.max_logs.max(1);
    
    let cache_dir = get_cache_dir();
    rotate_logs(&cache_dir, max_logs);

    if let Ok(cache) = LOG_CACHE.lock() {
        if cache.is_empty() { return; }

        let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S");
        let mut log_path = cache_dir.clone();
        log_path.push(format!("rmpt_{}.log", timestamp));

        if let Ok(mut file) = File::create(log_path) {
            for line in cache.iter() {
                let _ = writeln!(file, "{}", line);
            }
        }
    }
}

fn rotate_logs(path: &PathBuf, max: usize) {
    if let Ok(entries) = fs::read_dir(path) {
        let mut logs: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                if name.starts_with("rmpt_") && name.ends_with(".log") {
                    Some(e.path())
                } else {
                    None
                }
            })
            .collect();

        if logs.len() >= max {
            logs.sort();
            let to_remove = logs.len().saturating_sub(max - 1);
            for i in 0..to_remove {
                let _ = fs::remove_file(&logs[i]);
            }
        }
    }
}