use chrono::Local;

pub fn log(message: &str) {
    let timestamp = Local::now().format("%H:%M:%S");
    // Сейчас это просто принт, но так как все модули дергают только эту функцию,
    // замена на TUI-виджет займет 5 минут.
    println!("[{}] {}", timestamp, message);
}