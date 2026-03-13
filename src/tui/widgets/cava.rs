use ratatui::{
    Frame,
    layout::{Margin, Rect},
    style::Color,
};
use std::sync::{Mutex, OnceLock};

// Хранилище для инерции внутри самого виджета
static PREV_FREQS: OnceLock<Mutex<Vec<f32>>> = OnceLock::new();

pub fn draw_cava_widget(f: &mut Frame, area: Rect, raw_frequencies: &[f32]) {
    let conf = crate::config::config::Config::global();
    let ui = &conf.ui;

    if area.height < 2 || raw_frequencies.is_empty() {
        return;
    }

    let inner_area = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let width = inner_area.width as usize;
    let height = inner_area.height as usize;

    let mut prev_lock = PREV_FREQS
        .get_or_init(|| Mutex::new(vec![0.0; raw_frequencies.len()]))
        .lock()
        .unwrap();

    // Ограничиваем рабочий диапазон (отрезаем ультразвук и шум, берем первые 80% массива)
    let focus_range = (raw_frequencies.len() as f32 * 0.8) as usize;
    let data = &raw_frequencies[..focus_range];

    // --- 1. ПЕРЕРАСЧЕТ ЧАСТОТ С ЛОГАРИФМИЧЕСКИМ ШАГОМ ---
    let mut target_freqs = vec![0.0f32; width];

    for x in (0..width).step_by(3) {
        let start_pct = (x as f32 / width as f32).powi(2);
        let end_pct = ((x + 2) as f32 / width as f32).powi(2);

        let start_idx = ((start_pct * data.len() as f32) as usize).min(data.len() - 1);
        let end_idx = ((end_pct * data.len() as f32) as usize).clamp(start_idx + 1, data.len());

        let chunk = &data[start_idx..end_idx];
        let mut val = chunk.iter().fold(0.0f32, |m: f32, &v| m.max(v));

        // ПРИМЕНЯЕМ ГЕЙТ: если звук тише порога, то это 0
        if val < ui.cava_noise_gate {
            val = 0.0;
        } else {
            let pos = x as f32 / width as f32;
            let bell = (-(pos - 0.5).powi(2) * 6.0).exp();
            let total_boost = 1.0 + (ui.cava_tilt * bell);

            // Софт-клиппинг и усиление
            val = (val * ui.cava_sensitivity * total_boost * 1.5).tanh();
        }

        target_freqs[x] = val.powf(ui.cava_exponent).clamp(0.0, 1.0);
    }

    // --- 2. СГЛАЖИВАНИЕ И ИНЕРЦИЯ ---
    for i in (0..width).step_by(3) {
        let target = target_freqs[i];
        let prev = prev_lock[i];

        if target > prev {
            prev_lock[i] = prev + (target - prev) * ui.cava_attack;
        } else {
            prev_lock[i] = prev - (prev - target) * (1.0 - ui.cava_fall_speed);
        }
    }

    // --- 3. ОТРИСОВКА ---
    let symbols = ["▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let main_color = Color::Rgb(
        ui.colors.buttons[0],
        ui.colors.buttons[1],
        ui.colors.buttons[2],
    );
    let buffer = f.buffer_mut();

    for x_idx in (0..width).step_by(3) {
        let val = prev_lock[x_idx];
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
                symbols[(partial_level - 1).min(6)]
            } else if y == 0 {
                "▂"
            } else {
                break;
            };

            for offset in 0..2 {
                let cell_x = inner_area.left() + (x_idx + offset) as u16;
                if cell_x < inner_area.right() {
                    if let Some(cell) = buffer.cell_mut((cell_x, cell_y)) {
                        cell.set_symbol(sym).set_fg(main_color);
                    }
                }
            }
        }
    }
}
