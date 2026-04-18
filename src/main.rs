mod audio_engine;
mod config;
mod input;
mod logger;
mod parser;
mod tui;

use crate::audio_engine::engine::AudioEngine;
use crossterm::{
    cursor::{Hide, Show},
    event::{Event, EventStream},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::{StreamExt};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::env;
use std::io;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Инициализируем логгер в отдельном потоке
    let logger_handle = logger::init_logger();

    // Загружаем конфиг (может писать в логгер)
    let _ = crate::config::config::Config::global();

    if let Some(err) = crate::config::config::Config::get_last_error() {
        eprintln!("\x1b[31;1m[FATAL] Ошибка в конфигурации:\x1b[0m");
        eprintln!("\x1b[33m{}\x1b[0m", err);
        logger::log(&format!("FATAL: Config error: {}", err));
        logger::flush_and_exit();
        logger_handle.join().unwrap();
        std::process::exit(1);
    }

    // Устанавливаем паник-хендлер
    std::panic::set_hook(Box::new(|info| {
        let _ = disable_raw_mode();
        let mut stdout = std::io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, Show);
        logger::emergency_flush(info);
        // Принудительно завершаем процесс, не дожидаясь tokio
        std::process::abort();
    }));

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

    // --- Инициализация терминала ---
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;
    print!("\x1b[?1000l\x1b[?1003l");

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut engine = AudioEngine::new();

    if let Some(path) = initial_path {
        engine.play(&path).await;
    }

    let mut log_empty_sent = false;

    // --- Настройка FPS ---
    let tick_rate = Duration::from_millis(16);
    let mut last_activity = Instant::now();
    let mut needs_render = true;

    // Асинхронный поток событий crossterm
    let mut event_stream = EventStream::new().fuse();

    logger::log("MAIN: Entering main loop");

    loop {
        let is_paused = engine.is_paused();
        let is_empty = engine.is_empty();
        let is_animating = !is_paused && !is_empty;

        if is_animating {
            last_activity = Instant::now();
        }

        let should_render =
            is_animating || last_activity.elapsed() < Duration::from_secs(2) || needs_render;

        // Вычисляем таймаут для следующего рендера
        let next_tick = tick_rate.saturating_sub(last_activity.elapsed());

        tokio::select! {
            // Обработка событий ввода
            maybe_event = event_stream.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => {
                        needs_render = true;
                        if !crate::input::handle_input(&engine, key).await {
                            break;
                        }
                    }
                    Some(Ok(_)) => {} // Игнорируем другие события (мышь и т.п.)
                    Some(Err(e)) => {
                        logger::log(&format!("INPUT ERROR: {}", e));
                    }
                    None => break, // Поток событий закрыт
                }
            }
            // Таймер для отрисовки
            _ = tokio::time::sleep(next_tick) => {
                if should_render {
                    terminal.draw(|f| {
                        let size = f.area();
                        crate::tui::main_tab::draw_main_layout(f, size, &engine);
                    })?;
                    needs_render = false;
                }

                if is_empty && !log_empty_sent {
                    logger::log("MAIN: Engine idle");
                    log_empty_sent = true;
                } else if !is_empty {
                    log_empty_sent = false;
                }
            }
        }
    }

    // --- Выход ---
    logger::log("MAIN: Exiting...");
    engine.shutdown().await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, Show)?;

    // Завершаем логгер
    logger::flush_and_exit();
    logger_handle.join().unwrap();

    Ok(())
}
