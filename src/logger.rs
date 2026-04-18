use chrono::Local;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::mpsc::{self, Sender};
use std::thread;

static LOG_TX: OnceLock<Sender<LogCommand>> = OnceLock::new();

enum LogCommand {
    Log(String),
    FlushAndExit,
}

/// Инициализирует поток логирования. Возвращает JoinHandle, который нужно
/// дождаться при штатном завершении программы.
pub fn init_logger() -> thread::JoinHandle<()> {
    let (tx, rx) = mpsc::channel();
    LOG_TX.set(tx).unwrap();

    thread::spawn(move || {
        let cache_dir = get_cache_dir();
        let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S");
        let log_path = cache_dir.join(format!("rmpt_{}.log", timestamp));

        let mut file = File::create(&log_path).expect("Failed to create log file");
        let mut cache: Vec<String> = Vec::new();

        // Ротация старых логов при старте
        let max_logs = crate::config::config::Config::global().logging.max_logs;
        if let Err(e) = rotate_logs_sync(&cache_dir, max_logs) {
            eprintln!("Log rotation error: {}", e);
        }

        loop {
            match rx.recv() {
                Ok(LogCommand::Log(msg)) => {
                    cache.push(msg);
                    // Периодический сброс в файл
                    if cache.len() >= 50 {
                        for line in cache.drain(..) {
                            let _ = writeln!(file, "{}", line);
                        }
                        let _ = file.flush();
                    }
                }
                Ok(LogCommand::FlushAndExit) | Err(_) => {
                    // Записываем остатки и выходим
                    for line in cache.drain(..) {
                        let _ = writeln!(file, "{}", line);
                    }
                    let _ = file.flush();
                    eprintln!("Log saved to {:?}", log_path);
                    break;
                }
            }
        }
    })
}

/// Отправляет сообщение в логгер (без ожидания, мгновенно).
pub fn log(message: &str) {
    let timestamp = Local::now().format("%H:%M:%S");
    let formatted = format!("[{}] {}", timestamp, message);
    if let Some(tx) = LOG_TX.get() {
        let _ = tx.send(LogCommand::Log(formatted));
    }
}

/// Для штатного завершения: просим логгер дописать всё и выйти.
pub fn flush_and_exit() {
    if let Some(tx) = LOG_TX.get() {
        let _ = tx.send(LogCommand::FlushAndExit);
    }
}

/// Экстренный сброс при панике – вызывается из panic hook.
pub fn emergency_flush(panic_info: &std::panic::PanicHookInfo) {
    // Пытаемся отправить сигнал завершения и немного подождать
    if let Some(tx) = LOG_TX.get() {
        let _ = tx.send(LogCommand::FlushAndExit);
        // Даём потоку время на запись (в реальности паника всё равно убьёт процесс)
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // На всякий случай выводим в stderr
    eprintln!("\n!!! RMPT CRITICAL PANIC !!!");
    eprintln!("Reason: {}", panic_info);
}

fn get_cache_dir() -> PathBuf {
    let mut path = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("rmpt");
    let _ = std::fs::create_dir_all(&path);
    path
}

fn rotate_logs_sync(path: &PathBuf, max: usize) -> std::io::Result<()> {
    let mut logs: Vec<PathBuf> = std::fs::read_dir(path)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|s| s.starts_with("rmpt_") && s.ends_with(".log"))
                .unwrap_or(false)
        })
        .map(|e| e.path())
        .collect();

    if logs.len() >= max {
        logs.sort();
        let to_remove = logs.len().saturating_sub(max - 1);
        for path in logs.iter().take(to_remove) {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(())
}
