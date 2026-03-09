mod mpv_handler;
mod logger;
mod config;
mod parser;
mod tui;

use mpv_handler::{MpvController, PlayerEvent};
use logger::log;
use std::env;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Использование: rmpt <путь_к_музыке>");
        std::process::exit(1);
    }
    let target_path = &args[1];

    log(&format!("RMPT запущен. Цель: {}", target_path));

    let (player, mut event_rx) = MpvController::new();
    player.load(target_path); 

    loop {
        tokio::select! {
            Some(event) = event_rx.recv() => {
                match event {
                    PlayerEvent::TrackStarted => {
                        log("--- Новый трек начался ---");
                    }
                    PlayerEvent::TimePos(pos) => {
                        if (pos as u64) % 5 == 0 {
                            log(&format!("Прогресс: {:.0}с", pos));
                        }
                    }
                    PlayerEvent::MetadataUpdate { artist, title } => {
                        log(&format!("Метаданные: {} - {}", artist, title));
                    }
                    PlayerEvent::EndFile(reason) => {
                        log(&format!("Воспроизведение окончено. Причина: {}", reason));
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                log("Выход...");
                break;
            }
        }
    }
}