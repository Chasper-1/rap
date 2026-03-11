use chrono::Local;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::Mutex;
use std::sync::OnceLock;

static LOG_WRITER: OnceLock<Mutex<BufWriter<File>>> = OnceLock::new();

pub fn log(message: &str) {
    let timestamp = Local::now().format("%H:%M:%S");
    let log_line = format!("[{}] {}\n", timestamp, message);

    let writer_mutex = LOG_WRITER.get_or_init(|| {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open("rmpt.log")
            .expect("Failed to open log file");
        Mutex::new(BufWriter::with_capacity(8192, file)) // Буфер 8КБ
    });

    if let Ok(mut writer) = writer_mutex.lock() {
        let _ = writer.write_all(log_line.as_bytes());
    }
}

pub fn final_flush() {
    if let Some(writer_mutex) = LOG_WRITER.get() {
        if let Ok(mut writer) = writer_mutex.lock() {
            let _ = writer.flush();
        }
    }
}