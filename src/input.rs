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

    let mut current_volume = engine.get_volume();

    // 2. ДЕЙСТВИЯ
    match key {
        // ПАУЗА
        k if match_cfg(k, &cfg.toggle_pause, "PAUSE") => {
            if engine.is_paused() {
                engine.resume().await;
                crate::logger::log("AUDIO: Resumed");
            } else {
                engine.pause().await;
                crate::logger::log("AUDIO: Paused");
            }
        }

        // ГРОМКОСТЬ +
        k if match_cfg(k, &cfg.vol_up, "VOL_UP") => {
            current_volume = (current_volume + 0.05).min(1.0); // Сначала меняем СВОЮ переменную
            engine.set_volume(current_volume).await; // Потом тупо пушим её в движок
            crate::logger::log(&format!("AUDIO: Volume {:.2}", current_volume));
        }

        // ГРОМКОСТЬ -
        k if match_cfg(k, &cfg.vol_down, "VOL_DOWN") => {
            current_volume = (current_volume - 0.05).max(0.0); // Сначала меняем СВОЮ переменную
            engine.set_volume(current_volume).await; // Потом тупо пушим её в движок
            crate::logger::log(&format!("AUDIO: Volume {:.2}", current_volume));
        }

        // ВПЕРЕД
        k if match_cfg(k, &cfg.forward, "FORWARD") => {
            let step = cfg.forward_step;
            crate::logger::log(&format!("INPUT: Seek {:+}s", step));
            engine.seek_relative(step).await;

            let pos = engine.get_current_pos();
            crate::logger::log(&format!("AUDIO: Position {}s", pos));
        }

        // НАЗАД
        k if match_cfg(k, &cfg.backward, "BACKWARD") => {
            // На всякий случай гарантируем минус через .abs()
            let step = -(cfg.backward_step.abs());
            crate::logger::log(&format!("INPUT: Seek {:+}s", step));
            engine.seek_relative(step).await;

            let pos = engine.get_current_pos();
            crate::logger::log(&format!("AUDIO: Position {}s", pos));
        }

        // СТОП (Выгрузка трека)
        k if match_cfg(k, &cfg.stop, "STOP") => {
            engine.stop().await;
            crate::logger::log("AUDIO: Stopped and Cleared");
        }

        // СБРОС (Перемотка в 0)
        k if match_cfg(k, &cfg.seek_start, "SEEK_START") => {
            engine.seek_to(0).await;
            crate::logger::log("AUDIO: Reset to start");
        }

        _ => {}
    }

    true
}

fn match_cfg(code: KeyCode, keys: &[String], action_label: &str) -> bool {
    let is_match = keys.iter().any(|key_name| {
        let s = key_name.trim().to_lowercase();

        match code {
            // Если нажата обычная клавиша (буква/цифра)
            KeyCode::Char(c) => {
                let actual = c.to_lowercase().to_string();
                actual == s || (s == "space" && c == ' ')
            }
            // Если нажата спец-клавиша (Home, Enter, Up...)
            _ => {
                let code_str = format!("{:?}", code).to_lowercase();
                code_str == s
            }
        }
    });

    if is_match {
        // Мы логируем это, чтобы ты видел в логах: нажатие дошло!
        crate::logger::log(&format!("MATCH: {:?} matches {}", code, action_label));
    }
    is_match
}
