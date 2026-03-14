mod audio_engine;
mod config;
mod input;
mod logger;
mod parser;
mod tui;

use crate::audio_engine::engine::AudioEngine;
use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::env;
use std::io;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = crate::config::config::Config::global();

    if let Some(err) = crate::config::config::Config::get_last_error() {
        eprintln!("\x1b[31;1m[FATAL] Ошибка в конфигурации:\x1b[0m");
        eprintln!("\x1b[33m{}\x1b[0m", err);
        crate::logger::log(&format!("FATAL: Config error: {}", err));
        crate::logger::final_flush().await;
        std::process::exit(1);
    }

    // Паник-хендлер
    std::panic::set_hook(Box::new(|info| {
        let _ = disable_raw_mode();
        let mut stdout = std::io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, Show);
        eprintln!("\n\x1b[31;1m[FATAL ERROR]:\x1b[0m {}\n", info);
        crate::logger::emergency_flush(info);
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

    // 1. Входим в альт-экран и прячем курсор
    execute!(stdout, EnterAlternateScreen, Hide)?;

    // 2. ВЫКЛЮЧАЕМ ТОЛЬКО ВЫДЕЛЕНИЕ ТЕКСТА
    // Эти коды вырубают стандартное выделение мышкой в большинстве терминалов
    // при этом сама мышь (клики) может быть использована кодом, если надо.
    print!("\x1b[?1000l\x1b[?1003l");

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (engine, _rx) = AudioEngine::new();

    if let Some(path) = initial_path {
        engine.play(&path).await;
    }

    let mut log_empty_sent = false;

    // --- Настройка FPS ---
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(16); // 60 FPS
    crate::logger::log("MAIN: Entering main loop");
    let mut last_activity = Instant::now();
    let mut needs_render = true; // Флаг для разовой отрисовки по событию

    loop {
        let is_paused = engine.is_paused().await;
        let is_empty = engine.is_empty().await;
        let poll_timeout = tick_rate.saturating_sub(last_tick.elapsed());

        // 1. УСЛОВИЕ ЖИВОЙ АНИМАЦИИ
        // Если играет — мы активны. Если только что поставили на паузу — активны еще 2 сек.
        let is_animating = !is_paused && !is_empty;
        if is_animating {
            last_activity = Instant::now();
        }

        // 2. РЕШАЕМ: РИСОВАТЬ ИЛИ НЕТ?
        // Рисуем если: идет анимация ИЛИ мы в окне затухания (2 сек) ИЛИ флаг принудительной отрисовки
        let should_render =
            is_animating || last_activity.elapsed() < Duration::from_secs(2) || needs_render;

        // 3. ОПРОС СОБЫТИЙ
        if event::poll(poll_timeout)? {
            if let Event::Key(key) = event::read()? {
                needs_render = true; // Нажали кнопку — надо перерисовать интерфейс
                if !crate::input::handle_input(&engine, key).await {
                    break;
                }
            }
        }

        // 4. ИСПОЛНЕНИЕ
        if should_render {
            terminal.draw(|f| {
                let size = f.area();
                crate::tui::main_tab::draw_main_layout(f, size, &engine);
            })?;
            needs_render = false; // Отрисовали — сбросили флаг
        } else {
            // ВОТ ТУТ ОН ЗАТЫКАЕТСЯ.
            // Если анимации нет и 2 секунды прошли — спим глубоко.
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Таймер тиков (стандартно)
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();

            if is_empty {
                if !log_empty_sent {
                    crate::logger::log("MAIN: Engine idle");
                    log_empty_sent = true;
                }
            } else {
                log_empty_sent = false;
            }
        }
    }

    // --- Выход ---
    logger::final_flush().await;
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, Show)?;

    Ok(())
}
