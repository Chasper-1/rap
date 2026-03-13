use ratatui::{
    Frame,
    layout::{Margin, Rect},
    style::{Color},
};
use std::sync::{Mutex, OnceLock};

// Хранилище для инерции внутри самого виджета
static PREV_FREQS: OnceLock<Mutex<Vec<f32>>> = OnceLock::new();

pub fn draw_cava_widget(f: &mut Frame, area: Rect, raw_frequencies: &[f32]) {
    let conf = crate::config::config::Config::global();
    let ui = &conf.ui;

    if area.height < 2 || raw_frequencies.is_empty() { return; }

    let inner_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    let width = inner_area.width as usize;
    let height = inner_area.height as usize;

    let mut prev_lock = PREV_FREQS.get_or_init(|| Mutex::new(vec![0.0; raw_frequencies.len()])).lock().unwrap();
    if prev_lock.len() != raw_frequencies.len() {
        *prev_lock = vec![0.0; raw_frequencies.len()];
    }

    let avg_energy: f32 = raw_frequencies.iter().sum::<f32>() / raw_frequencies.len() as f32;

    // --- ШАГ 1: БАЗОВАЯ ОБРАБОТКА ---
    let mut processed: Vec<f32> = raw_frequencies.iter().enumerate().map(|(i, &raw_val)| {
        let pos = i as f32 / raw_frequencies.len() as f32;
        let edge_taper = (pos * std::f32::consts::PI).sin().powf(0.25);
        let bell = (-(pos - 0.5).powi(2) * 10.0).exp();
        let total_boost = edge_taper * (1.0 + (ui.cava_tilt * bell));
        
        (raw_val * ui.cava_sensitivity * total_boost).powf(ui.cava_exponent)
    }).collect();

    // --- ШАГ 2: ЧАСТОТНОЕ СГЛАЖИВАНИЕ (как в настоящей CAVA) ---
    // Соседние столбики делятся энергией, чтобы не было "зубьев"
    for i in 1..processed.len() - 1 {
        processed[i] = (processed[i-1] + processed[i] * 2.0 + processed[i+1]) / 4.0;
    }

    // --- ШАГ 3: ВРЕМЕННАЯ ИНЕРЦИЯ (Атака/Спад) ---
    for i in 0..processed.len() {
        let target = processed[i];
        let prev = prev_lock[i];
        
        if avg_energy < ui.cava_noise_gate {
            prev_lock[i] = (prev * ui.cava_fall_speed).max(0.0);
        } else if target > prev {
            prev_lock[i] = prev + (target - prev) * ui.cava_attack;
        } else {
            prev_lock[i] = (prev * ui.cava_fall_speed).max(0.0);
        }
    }

    // --- ШАГ 4: ОТРИСОВКА ---
    let symbols = ["▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let main_color = Color::Rgb(ui.colors.buttons[0], ui.colors.buttons[1], ui.colors.buttons[2]);
    let buffer = f.buffer_mut();

    for x_idx in (0..width).step_by(3) {
        let freq_idx = (x_idx * processed.len()) / width;
        let val = prev_lock[freq_idx];

        let total_levels = (val * height as f32 * 8.0) as usize;
        let full_blocks = total_levels / 8;
        let partial_level = total_levels % 8;

        for y in 0..height {
            let cell_y = inner_area.bottom().saturating_sub(1 + y as u16);
            if cell_y < inner_area.top() { break; }

            let sym = if y < full_blocks {
                "█"
            } else if y == full_blocks && partial_level > 0 {
                symbols[(partial_level - 1).min(6)]
            } else if y == 0 {
                "▂" // Фундамент, горит всегда
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