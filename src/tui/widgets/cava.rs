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

    // 1. УЛУЧШЕННАЯ ОБРАБОТКА (как в оригинальной CAVA)
    let frequencies: Vec<f32> = raw_frequencies
        .iter()
        .enumerate()
        .map(|(i, &current_val)| {
            // Применяем корень, чтобы "сплющить" динамический диапазон
            // (тихие звуки станут видны, громкие перестанут биться в потолок)
            let mut val = current_val.sqrt() * 1.2;

            // Частотное выравнивание (Bell-curve)
            // Придавливаем края (бас и ультразвук), акцентируем середину
            let len = raw_frequencies.len() as f32;
            let pos = i as f32 / len;
            let bell_weight = 0.5 + 0.5 * (-(pos - 0.5).powi(2) * 4.0).exp();
            val *= bell_weight;

            // Noise gate + минимальный порог
            if val < 0.02 {
                val = 0.01;
            }

            let prev_val = prev_lock[i];
            let new_val = if val > prev_val {
                val
            } else {
                prev_val * fall_speed
            };

            prev_lock[i] = new_val;
            new_val
        })
        .collect();

    let symbols = [" ", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let mut cava_content = String::with_capacity(width * height * 4);

    for h_idx in (0..height).rev() {
        let mut line = String::with_capacity(width * 4);
        let mut i = 0;

        while i < width {
            let data_idx = (i * frequencies.len()) / width;
            let mut val = *frequencies.get(data_idx).unwrap_or(&0.01);

            // Всегда рисуем хотя бы минимальную полоску на нижней строке
            if h_idx == 0 && val < 0.05 {
                val = 0.05;
            }

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

            let symbol = symbols[char_idx];

            // ОПТИМИЗАЦИЯ: Если это пустая строка (выше уровня звука),
            // мы могли бы вообще не рисовать, но для Paragraph нам нужны пробелы
            line.push_str(symbol);
            if i + 1 < width {
                line.push_str(symbol);
            }
            if i + 2 < width {
                line.push(' ');
            }

            i += 3;
        }

        cava_content.push_str(&line);
        if h_idx > 0 {
            cava_content.push('\n');
        }
    }

    let [r, g, b] = conf.ui.colors.buttons;
    f.render_widget(
        Paragraph::new(cava_content).style(Style::default().fg(Color::Rgb(r, g, b))),
        inner_area,
    );
}
