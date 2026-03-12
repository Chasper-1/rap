use crate::audio_engine::AudioEngine;
use crate::config::config::Config;
use crossterm::event::{KeyCode, KeyEvent};

pub async fn handle_input(engine: &AudioEngine, event: KeyEvent) -> bool {
    let cfg = &Config::global().input;
    let key = event.code;

    // СРАЗУ ЛОГ: ловим вообще любое нажатие
    crate::logger::log(&format!("DEBUG: Received key: {:?}", key));

    // 1. ВЫХОД (проверяем первым)
    if match_cfg(key, &cfg.quit, "QUIT") {
        crate::logger::log("INPUT: Exiting application...");
        return false;
    }

    // 2. УПРАВЛЕНИЕ (через match)
    match key {
        // ПАУЗА
        k if match_cfg(k, &cfg.toggle_pause, "PAUSE") => {
            if engine.is_paused().await {
                crate::logger::log("INPUT: Resuming...");
                engine.resume().await;
            } else {
                crate::logger::log("INPUT: Pausing...");
                engine.pause().await;
            }
        }

        // ГРОМКОСТЬ ВВЕРХ
        k if match_cfg(k, &cfg.vol_up, "VOL_UP") => {
            let current = engine.get_volume().await;
            let target = (current + 0.05).min(1.0);
            crate::logger::log(&format!("INPUT: Vol UP ({:.2} -> {:.2})", current, target));
            engine.set_volume(target).await;
        }

        // ГРОМКОСТЬ ВНИЗ
        k if match_cfg(k, &cfg.vol_down, "VOL_DOWN") => {
            let current = engine.get_volume().await;
            let target = (current - 0.05).max(0.0);
            crate::logger::log(&format!("INPUT: Vol DOWN ({:.2} -> {:.2})", current, target));
            engine.set_volume(target).await;
        }

        // ПЕРЕМОТКА ВПЕРЕД
        k if match_cfg(k, &cfg.forward, "FORWARD") => {
            crate::logger::log("INPUT: Command SEEK +10s");
            engine.seek_relative(10).await;
            let p = engine.get_current_pos().await;
            crate::logger::log(&format!("AUDIO: New position: {}s", p));
        }

        // ПЕРЕМОТКА НАЗАД
        k if match_cfg(k, &cfg.backward, "BACKWARD") => {
            crate::logger::log("INPUT: Command SEEK -10s");
            engine.seek_relative(-10).await;
            let p = engine.get_current_pos().await;
            crate::logger::log(&format!("AUDIO: New position: {}s", p));
        }

        // ВЕРНУЛ СУКА (Home или кнопка 0)
        KeyCode::Home | KeyCode::Char('0') => {
            crate::logger::log("INPUT: Resetting to START (seek_to 0)");
            engine.seek_to(0).await;
            crate::logger::log("AUDIO: Position reset to 0");
        }

        // Статус-чек
        KeyCode::Char('p') | KeyCode::Char('з') => {
            let pos = engine.get_current_pos().await;
            crate::logger::log(&format!("DEBUG: Manual check. Current pos: {}s", pos));
        }

        // Лог для всех остальных кнопок, которые мы не обработали
        _ => {
            crate::logger::log(&format!("INPUT: No action mapped for {:?}", key));
        }
    }

    true
}

fn match_cfg(code: KeyCode, cfg_str: &str, action_label: &str) -> bool {
    if cfg_str.trim().is_empty() {
        return false;
    }

    let is_match = cfg_str.split('|').any(|part| {
        let s = part.trim().to_lowercase();
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
            "-"     => code == KeyCode::Char('-'),
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
        crate::logger::log(&format!("MATCH: Key {:?} -> Action {}", code, action_label));
    }
    is_match
}