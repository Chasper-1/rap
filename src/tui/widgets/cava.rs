use ratatui::{
    Frame,
    layout::{Margin, Rect},
    style::{Color, Style},
    widgets::Paragraph,
};
use std::sync::{Mutex, OnceLock};

// Хранилище для инерции внутри самого виджета
static PREV_FREQS: OnceLock<Mutex<Vec<f32>>> = OnceLock::new();

pub fn draw_cava_widget(f: &mut Frame, area: Rect, raw_frequencies: &[f32]) {
    let conf = crate::config::config::Config::global();
    // 1. Берем из конфига и зажимаем в рамках разумного (0.0 - 1.0)
    let fall_speed = conf.ui.cava_fall_speed.clamp(0.0, 1.0);

    if area.height < 3 {
        return;
    }

    let inner_area = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let width = inner_area.width as usize;
    let height = inner_area.height as usize;

    let mut prev_lock = PREV_FREQS
        .get_or_init(|| Mutex::new(vec![0.0; 512]))
        .lock()
        .unwrap();

    if prev_lock.len() != raw_frequencies.len() {
        *prev_lock = vec![0.0; raw_frequencies.len()];
    }

    // 2. Обработка инерции (fall_speed теперь точно виден внутри)
    let frequencies: Vec<f32> = raw_frequencies
        .iter()
        .enumerate()
        .map(|(i, &current_val)| {
            let prev_val = prev_lock[i];
            let new_val = if current_val > prev_val {
                current_val
            } else {
                // Вот здесь была ошибка, теперь fall_speed доступен
                prev_val * fall_speed
            };
            prev_lock[i] = new_val;
            new_val
        })
        .collect();

    // --- ОТРИСОВКА ---
    let symbols = [" ", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let mut cava_content = String::with_capacity(width * height * 4);

    for h_idx in (0..height).rev() {
        let mut line = String::with_capacity(width * 4);

        for i in 0..width {
            let val = if frequencies.is_empty() {
                if h_idx == 0 { 0.05 } else { 0.0 }
            } else {
                let data_idx = (i * frequencies.len()) / width;
                *frequencies.get(data_idx).unwrap_or(&0.0)
            };

            let level_min = h_idx as f32 / height as f32;
            let level_max = (h_idx + 1) as f32 / height as f32;

            let char_idx = if val >= level_max {
                7
            } else if val > level_min {
                let internal_factor = (val - level_min) / (level_max - level_min);
                ((internal_factor * 8.0) as usize).min(7)
            } else {
                0
            };

            line.push_str(symbols[char_idx]);
        }
        cava_content.push_str(&line);
        if h_idx > 0 {
            cava_content.push('\n');
        }
    }

    f.render_widget(
        Paragraph::new(cava_content).style(Style::default().fg(Color::Cyan)),
        inner_area,
    );
}
