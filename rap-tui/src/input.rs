use crossterm::event::{self, Event};
use rap_engine::tokio::sync::mpsc;

// Запускает отдельный поток чтения событий терминала
// (crossterm::event::read блокирующий) и отдаёт канал событий.
pub fn spawn_event_reader() -> mpsc::UnboundedReceiver<Event> {
    let (tx, rx) = mpsc::unbounded_channel();
    std::thread::spawn(move || {
        while let Ok(event) = event::read() {
            if tx.send(event).is_err() {
                break;
            }
        }
    });
    rx
}
