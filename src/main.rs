mod audio_engine;
mod config;
mod logger;
mod parser;
mod tui;

use audio_engine::AudioEngine;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::env;
use std::io;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Паник-хендлер (по твоему плану)
    std::panic::set_hook(Box::new(|info| {
        crate::logger::log(&format!("CRITICAL PANIC: {}", info));
        crate::logger::final_flush();
    }));

    // 2. Аргументы теперь опциональны
    let args: Vec<String> = env::args().skip(1).collect();
    let initial_path = if !args.is_empty() {
        let path = args.join(" ");
        if std::path::Path::new(&path).exists() {
            Some(path)
        } else {
            None
        }
    } else {
        None
    };

    // 3. Входим в TUI сразу
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 4. Инициализация движка (он в спячке, пока не позовем play)
    let engine = AudioEngine::new();

    // Если путь всё-таки передали и он валидный — запускаем
    if let Some(path) = initial_path {
        engine.play(&path).await;
    }

    let mut log_empty_sent = false;

    // 5. Главный цикл
    loop {
        terminal.draw(|f| {
            let size = f.area();
            crate::tui::main_tab::draw_main_layout(f, size);
        })?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                // Выход на 'q'
                if key.code == KeyCode::Char('q') {
                    break;
                }

                // Сюда потом добавим логику выбора файла из библиотеки и запуск через engine.play()
            }
            if engine.is_empty().await {
                if !log_empty_sent {
                    crate::logger::log("Audio engine is idle (queue empty)");
                    log_empty_sent = true;
                }
            } else {
                log_empty_sent = false; // сбрасываем, если что-то заиграло
            }
        }

        // Больше не выходим автоматически, если движок пуст.
        // Теперь мы в плеере, даже если тишина.
    }

    if let Some(err) = crate::config::config::Config::get_last_error() {
        println!("\x1b[31;1m[!] Ошибки конфига при запуске:\x1b[0m");
        println!("\x1b[33m{}\x1b[0m", err);
    }

    // 6. Финал
    logger::final_flush();
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
