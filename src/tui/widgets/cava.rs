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
    let _ = ui.cava_update_ms; 

    if area.height < 2 || raw_frequencies.is_empty() { return; }

    let inner_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    let width = inner_area.width as usize;
    let height = inner_area.height as usize;

    let mut prev_lock = PREV_FREQS
            .get_or_init(|| Mutex::new(vec![0.0; 512]))
            .lock()
            .unwrap();

    let avg_energy: f32 = raw_frequencies.iter().sum::<f32>() / raw_frequencies.len() as f32;

    // 1. ПОДГОТОВКА ДАННЫХ
    let frequencies: Vec<f32> = raw_frequencies
        .iter()
        .enumerate()
        .map(|(i, &raw_val)| {
            let prev_val = prev_lock[i];
            if avg_energy < ui.cava_noise_gate {
                let dropped = (prev_val * ui.cava_fall_speed).max(0.0);
                prev_lock[i] = dropped;
                return dropped;
            }
            let pos = i as f32 / raw_frequencies.len() as f32;
            let edge_taper = (pos * std::f32::consts::PI).sin().powf(0.25);
            let bell = (-(pos - 0.5).powi(2) * 10.0).exp();
            let total_boost = edge_taper * (1.0 + (ui.cava_tilt * bell));
            let target = (raw_val * ui.cava_sensitivity * total_boost).powf(ui.cava_exponent);
            let final_val = if target > prev_val {
                prev_val + (target - prev_val) * ui.cava_attack
            } else {
                (prev_val * ui.cava_fall_speed).max(0.0)
            };
            prev_lock[i] = final_val.clamp(0.0, 1.0);
            prev_lock[i]
        })
        .collect();

    // 2. ОТРИСОВКА В БУФЕР (БЕЗ ДЕПРЕКЕЙТЕД МЕТОДОВ)
    let symbols = ["▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let main_color = Color::Rgb(ui.colors.buttons[0], ui.colors.buttons[1], ui.colors.buttons[2]);
    let buffer = f.buffer_mut();

    for x in 0..width {
        if x % 3 == 2 { continue; } // Пропуск для зазора

        let start = (x * frequencies.len()) / width;
        let end = ((x + 1) * frequencies.len()) / width;
        let val = frequencies[start..end.max(start + 1)].iter().sum::<f32>() / (end - start).max(1) as f32;

        if val <= 0.001 { continue; }

        let total_symbols = (val * height as f32 * 8.0) as usize;
        let full_blocks = total_symbols / 8;
        let partial_block = total_symbols % 8;

        for y in 0..height {
            if y > full_blocks { break; } // Дальше рисовать нечего

            let cell_y = inner_area.bottom().saturating_sub(1 + y as u16);
            let cell_x = inner_area.left() + x as u16;

            if cell_y < inner_area.top() { break; }

            if y < full_blocks {
                // Используем новый синтаксис доступа к ячейке
                if let Some(cell) = buffer.cell_mut((cell_x, cell_y)) {
                    cell.set_symbol("█").set_fg(main_color);
                }
            } else if partial_block > 0 {
                let sym = symbols[(partial_block - 1).min(6)];
                if let Some(cell) = buffer.cell_mut((cell_x, cell_y)) {
                    cell.set_symbol(sym).set_fg(main_color);
                }
            }
        }
    }
}