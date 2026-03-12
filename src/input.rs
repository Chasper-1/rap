use crate::audio_engine::AudioEngine;
use crate::config::config::Config;
use crossterm::event::{KeyCode, KeyEvent};

pub async fn handle_input(engine: &AudioEngine, event: KeyEvent) -> bool {
    let cfg = &Config::global().input;
    let key = event.code;

    // 1. Выход (берем из конфига)
    if match_cfg(key, &cfg.quit) {
        return false;
    }

    // 2. Управление движком
    match key {
        // 2. Везде добавляем .await, иначе это просто "обещание" выполнить код
        k if match_cfg(k, &cfg.toggle_pause) => {
            if engine.is_paused().await {
                engine.resume().await;
            } else {
                engine.pause().await;
            }
        }

        k if match_cfg(k, &cfg.vol_up) => {
            let current = engine.get_volume().await;
            engine.set_volume(current + 0.05).await;
        }

        k if match_cfg(k, &cfg.vol_down) => {
            let current = engine.get_volume().await;
            engine.set_volume(current - 0.05).await;
        }

        k if match_cfg(k, &cfg.forward) => {
            engine.seek_relative(10).await;
            // Используем метод здесь, чтобы видеть куда прыгнули
            let _pos = engine.get_current_pos().await;
        }

        k if match_cfg(k, &cfg.backward) => {
            engine.seek_relative(-10).await;
            let _pos = engine.get_current_pos().await;
        }

        // Кнопки для чистки варнингов
        KeyCode::Home | KeyCode::Char('0') => {
            engine.seek_to(0).await;
        }

        // Явный вызов позиции (например для отладки в логах)
        KeyCode::Char('p') => {
            let pos = engine.get_current_pos().await;
            crate::logger::log(&format!("Audio: Current position {}s", pos));
        }

        k if match_cfg(k, &cfg.forward) => {
            engine.seek_relative(5).await; // Прыжок на +10 секунд
        }

        // ПЕРЕМОТКА НАЗАД (Стрелка влево)
        k if match_cfg(k, &cfg.backward) => {
            engine.seek_relative(-5).await; // Прыжок на -10 секунд
        }

        _ => {}
    }

    true
}

/// Проверяет совпадение клавиши. Не паникует, если конфиг пустой.
fn match_cfg(code: KeyCode, cfg_str: &str) -> bool {
    // Если в конфиге пусто — это не ошибка, просто это действие не сработает
    if cfg_str.trim().is_empty() {
        return false;
    }

    cfg_str.split('|').any(|part| {
        let s = part.trim();
        match s {
            "Space" | " " => code == KeyCode::Char(' '),
            "Up" => code == KeyCode::Up,
            "Down" => code == KeyCode::Down,
            "Left" => code == KeyCode::Left,
            "Right" => code == KeyCode::Right,
            "Enter" => code == KeyCode::Enter,
            "Esc" => code == KeyCode::Esc,
            "Home" => code == KeyCode::Home,
            "=" => code == KeyCode::Char('='),
            "+" => code == KeyCode::Char('+'),
            "-" => code == KeyCode::Char('-'),
            _ if s.len() == 1 => {
                let c = s.chars().next().unwrap_or(' ');
                code == KeyCode::Char(c)
            }
            _ => false,
        }
    })
}
