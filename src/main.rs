mod audio_engine;
mod logger;
mod config;
mod parser;

use audio_engine::AudioEngine;
use std::env;
use std::io;
use std::time::Duration;
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("ERROR: Usage: rmpt <path>");
        return Ok(());
    }
    let path = args.join(" ");

    if !std::path::Path::new(&path).exists() {
        eprintln!("ERROR: Файл не найден: {}", path);
        return Ok(());
    }

    // 1. Входим в режим TUI (Alternate Screen + Raw Mode)
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 2. Запускаем движок
    let engine = AudioEngine::new();
    engine.play(&path).await;

    // 3. Главный цикл отрисовки
    loop {
        // РИСУЕМ ТВОЮ СЕТКУ ИЗ ПОЛОСОК
        terminal.draw(|f| {
            let size = f.area();
            // Твоя функция из parser/main_tab.rs
            crate::parser::main_tab::draw_main_layout(f, size);
        })?;

        // Слушаем нажатия клавиш
        // poll(0) позволяет циклу крутиться без задержек для плавности
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }

        // Если трек закончился — выходим
        if engine.is_empty().await {
            break;
        }
    }
    logger::final_flush();
    
    // 4. Восстанавливаем терминал в исходное состояние
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    
    Ok(())
}
