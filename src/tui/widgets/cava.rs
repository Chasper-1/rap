use ratatui::{
    Frame,
    layout::{Margin, Rect},
    style::Color,
};

use std::sync::{Mutex, OnceLock};
static PREV_LEVELS: OnceLock<Mutex<Vec<f32>>> = OnceLock::new();

pub fn draw_cava_widget(f: &mut Frame, area: Rect, frequencies: &[f32]) {
    let conf = crate::config::config::Config::global();
    let ui = &conf.ui;

    // Убрали frequencies.is_empty(), чтобы он мог рисовать падение на паузе
    if area.height < 2 {
        return;
    }

    let inner_area = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let width = inner_area.width as usize;
    let height = inner_area.height as usize;
    let symbols = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let main_color = Color::Rgb(
        ui.colors.buttons[0],
        ui.colors.buttons[1],
        ui.colors.buttons[2],
    );
    let buffer = f.buffer_mut();

    let num_bars = width / 3;
    if num_bars == 0 {
        return;
    }

    let prev_levels_mutex = PREV_LEVELS.get_or_init(|| Mutex::new(Vec::new()));
    let mut prev_levels = prev_levels_mutex.lock().unwrap();
    if prev_levels.len() != num_bars {
        prev_levels.resize(num_bars, 0.0);
    }

    for i in 0..num_bars {
        let freq_idx = (i * frequencies.len()) / num_bars;
        let target_val = frequencies.get(freq_idx).cloned().unwrap_or(0.0);

        // Падение: берем максимум между новым значением и "упавшим" старым
        let prev = prev_levels[i];
        let val = if target_val > prev {
            // Плавный взлет (атака)
            prev + (target_val - prev) * ui.cava_attack
        } else {
            // Плавное падение (fall_speed)
            (prev * ui.cava_fall_speed).max(0.0)
        };
        prev_levels[i] = val;

        let x_pos = i * 3;
        let total_levels = (val * height as f32 * 8.0) as usize;
        let full_blocks = total_levels / 8;
        let partial_level = total_levels % 8;

        for y in 0..height {
            let cell_y = inner_area.bottom().saturating_sub(1 + y as u16);
            if cell_y < inner_area.top() {
                break;
            }
            let sym = if y < full_blocks {
                "█"
            } else if y == full_blocks && partial_level > 0 {
                symbols[(partial_level - 1).min(7)]
            } else if y == 0 {
                "▁"
            } else {
                break;
            };

            for offset in 0..2 {
                let cell_x = inner_area.left() + (x_pos + offset) as u16;
                if cell_x < inner_area.right() {
                    if let Some(cell) = buffer.cell_mut((cell_x, cell_y)) {
                        cell.set_symbol(sym).set_fg(main_color);
                    }
                }
            }
        }
    }
}
