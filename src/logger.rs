use std::fs::OpenOptions;
use std::io::Write;
use chrono::Local;

pub fn log(message: &str) {
    let timestamp = Local::now().format("%H:%M:%S");
    let log_message = format!("[{}] {}\n", timestamp, message);

    // Печатаем в stdout пока нет TUI
    print!("{}", log_message);

    // И дублируем в файл rmpt.log
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("rmpt.log") 
    {
        let _ = file.write_all(log_message.as_bytes());
    }
}