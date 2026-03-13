mod audio_engine;
mod config;
mod logger;
mod parser;
mod tui;
mod input;

use crate::audio_engine::engine::AudioEngine;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::env;
use std::io;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Сначала инициализируем конфиг и проверяем его на вшивость
    let _ = crate::config::config::Config::global();

    if let Some(err) = crate::config::config::Config::get_last_error() {
        // Если конфиг битый — СРАЗУ ГОВОРИМ ОБ ЭТОМ И ВЫХОДИМ
        // Не заходим в TUI, не портим терминал
        eprintln!("\x1b[31;1m[FATAL] Ошибка в конфигурации:\x1b[0m");
        eprintln!("\x1b[33m{}\x1b[0m", err);

        // Пишем в логгер и сбрасываем на диск перед выходом
        crate::logger::log(&format!("FATAL: Config error: {}", err));
        crate::logger::final_flush().await;

        std::process::exit(1);
    }

    // 1. Паник-хендлер (по твоему плану)
    std::panic::set_hook(Box::new(|info| {
        // Сначала ПРИНУДИТЕЛЬНО возвращаем терминал в нормальный режим
        // Делаем это через стандартный вывод, игнорируя ошибки
        let _ = crossterm::terminal::disable_raw_mode();
        let mut stdout = std::io::stdout();
        let _ = crossterm::execute!(stdout, crossterm::terminal::LeaveAlternateScreen, crossterm::cursor::Show);
    
        // Теперь пишем в логгер, что именно случилось
        crate::logger::log(&format!("CRITICAL PANIC: {}", info));
        
        // Сбрасываем логи на диск (через хендл текущего рантайма, как мы делали)
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.block_on(crate::logger::final_flush());
        }
    
        // Печатаем саму ошибку в чистый терминал, чтобы ты её видел
        eprintln!("\n\x1b[31;1m[FATAL ERROR]:\x1b[0m {}\n", info);
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
    let (engine, _rx) = AudioEngine::new();

    // Если путь всё-таки передали и он валидный — запускаем
    if let Some(path) = initial_path {
        engine.play(&path).await;
    }

    let mut log_empty_sent = false;

    // 5. Главный цикл
    loop {
            terminal.draw(|f| {
                let size = f.area();
                crate::tui::main_tab::draw_main_layout(f, size, &engine);
            })?;
    
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    // Вызываем процессор: он сам лезет в конфиг, дёргает методы движка 
                    // и говорит нам, когда пора выходить (если нажата кнопка выхода)
                    if !crate::input::handle_input(&engine, key).await {
                        break;
                    }
                }
    
                if engine.is_empty().await {
                    if !log_empty_sent {
                        crate::logger::log("Audio engine is idle (queue empty)");
                        log_empty_sent = true;
                    }
                } else {
                    log_empty_sent = false; 
                }
            }
        }

    if let Some(err) = crate::config::config::Config::get_last_error() {
        println!("\x1b[31;1m[!] Ошибки конфига при запуске:\x1b[0m");
        println!("\x1b[33m{}\x1b[0m", err);
    }

    // 6. Финал
    logger::final_flush().await;
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
