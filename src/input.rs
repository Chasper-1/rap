use crate::audio_engine::AudioEngine;
use crate::config::config::Config;
use crossterm::event::{KeyCode, KeyEvent};

pub async fn handle_input(engine: &AudioEngine, event: KeyEvent) -> bool {
    let cfg = &Config::global().input;
    let key = event.code;

    // Лог нажатия для дебага
    crate::logger::log(&format!("DEBUG: Key pressed: {:?}", key));

    // 1. ВЫХОД
    if match_cfg(key, &cfg.quit, "QUIT") {
        crate::logger::log("INPUT: Exiting...");
        return false;
    }

    // 2. ДЕЙСТВИЯ
    match key {
        // ПАУЗА
        k if match_cfg(k, &cfg.toggle_pause, "PAUSE") => {
            if engine.is_paused().await {
                engine.resume().await;
                crate::logger::log("AUDIO: Resumed");
            } else {
                engine.pause().await;
                crate::logger::log("AUDIO: Paused");
            }
        }

        // ГРОМКОСТЬ +
        k if match_cfg(k, &cfg.vol_up, "VOL_UP") => {
            let target = (engine.get_volume().await + 0.05).min(1.0);
            engine.set_volume(target).await;
            crate::logger::log(&format!("AUDIO: Volume {:.2}", target));
        }

        // ГРОМКОСТЬ -
        k if match_cfg(k, &cfg.vol_down, "VOL_DOWN") => {
            let target = (engine.get_volume().await - 0.05).max(0.0);
            engine.set_volume(target).await;
            crate::logger::log(&format!("AUDIO: Volume {:.2}", target));
        }

        // ВПЕРЕД
        k if match_cfg(k, &cfg.forward, "FORWARD") => {
            let step = cfg.forward_step;
            crate::logger::log(&format!("INPUT: Seek {:+}s", step));
            engine.seek_relative(step).await;
            
            let pos = engine.get_current_pos().await;
            crate::logger::log(&format!("AUDIO: Position {}s", pos));
        }

        // НАЗАД
        k if match_cfg(k, &cfg.backward, "BACKWARD") => {
            // На всякий случай гарантируем минус через .abs()
            let step = -(cfg.backward_step.abs());
            crate::logger::log(&format!("INPUT: Seek {:+}s", step));
            engine.seek_relative(step).await;
            
            let pos = engine.get_current_pos().await;
            crate::logger::log(&format!("AUDIO: Position {}s", pos));
        }

        // СБРОС (Home / 0)
        KeyCode::Home | KeyCode::Char('0') => {
            engine.seek_to(0).await;
            crate::logger::log("AUDIO: Reset to start");
        }

        _ => {}
    }

    true
}

/// Сверяет KeyCode со списком строк из конфига
fn match_cfg(code: KeyCode, keys: &[String], action_label: &str) -> bool {
    let is_match = keys.iter().any(|key_name| {
        let s = key_name.trim().to_lowercase();
        match s.as_str() {
            "space" | " " => code == KeyCode::Char(' '),
            "up"    => code == KeyCode::Up,
            "down"  => code == KeyCode::Down,
            "left"  => code == KeyCode::Left,
            "right" => code == KeyCode::Right,
            "enter" => code == KeyCode::Enter,
            "esc"   => code == KeyCode::Esc,
            "home"  => code == KeyCode::Home,
            "+" | "=" => code == KeyCode::Char('+') || code == KeyCode::Char('='),
            "-" | "_" => code == KeyCode::Char('-') || code == KeyCode::Char('_'),
            
            // Если в массиве строка из 1 символа (буква/цифра)
            _ if s.chars().count() == 1 => {
                let target = s.chars().next().unwrap();
                if let KeyCode::Char(actual) = code {
                    actual.to_lowercase().next() == Some(target)
                } else {
                    false
                }
            }
            _ => false,
        }
    });

    if is_match {
        crate::logger::log(&format!("MATCH: {:?} matches {}", code, action_label));
    }
    is_match
}