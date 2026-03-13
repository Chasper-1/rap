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

    if area.height < 2 { return; }

    let inner_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    let width = inner_area.width as usize;
    let height = inner_area.height as usize;

    let mut prev_lock = PREV_FREQS
        .get_or_init(|| Mutex::new(vec![0.0f32; 2048]))
        .lock()
        .unwrap();

    if prev_lock.len() < width { prev_lock.resize(width + 10, 0.0); }

    // 1. ОГРАНИЧИВАЕМ ДИАПАЗОН (Выкидываем мертвый низ и пустой верх)
    // Берем диапазон от 2% до 70% от всего FFT — там самая жизнь
    let start_offset = (raw_frequencies.len() as f32 * 0.02) as usize;
    let end_offset = (raw_frequencies.len() as f32 * 0.7) as usize;
    
    let active_data = if raw_frequencies.len() > end_offset {
        &raw_frequencies[start_offset..end_offset]
    } else {
        &[0.0f32]
    };

    // 2. РАВНОМЕРНОЕ РАСПРЕДЕЛЕНИЕ
    let mut target_freqs = vec![0.0f32; width];
    let is_silent = raw_frequencies.is_empty() || active_data.iter().sum::<f32>() < 0.001;

    if !is_silent {
        for x in (0..width).step_by(3) {
            // Линейно берем кусок данных для каждого столбика
            let start = (x * active_data.len()) / width;
            let end = ((x + 2) * active_data.len()) / width;
            let chunk = &active_data[start..end.max(start + 1)];
            
            // Ищем пик в этом куске
            let mut val = chunk.iter().fold(0.0f32, |m: f32, &v| m.max(v));

            if val > ui.cava_noise_gate {
                let pos = x as f32 / width as f32;
                let bell = (-(pos - 0.5).powi(2) * 4.0).exp();
                let total_boost = 1.0 + (ui.cava_tilt * bell);
                
                // Усиление и мягкий лимитер
                val = (val * ui.cava_sensitivity * total_boost * 1.5).tanh();
                target_freqs[x] = val.powf(ui.cava_exponent).clamp(0.0, 1.0);
            }
        }
    }

    // 3. ФИЗИКА ПАДЕНИЯ (Блокируем возврат при паузе)
    for i in 0..width {
        let target = target_freqs[i];
        let prev = prev_lock[i];

        if is_silent {
            // Если музыка на паузе — только падаем
            prev_lock[i] = (prev * ui.cava_fall_speed).max(0.0);
        } else if target > prev {
            // Взлет
            prev_lock[i] = prev + (target - prev) * ui.cava_attack;
        } else {
            // Падение
            prev_lock[i] = (prev * ui.cava_fall_speed).max(target);
        }
        
        // Отрезаем микро-значения, чтобы не "мерцало" в нуле
        if prev_lock[i] < 0.001 { prev_lock[i] = 0.0; }
    }

    // 4. ОТРИСОВКА
    let symbols = ["▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let main_color = Color::Rgb(ui.colors.buttons[0], ui.colors.buttons[1], ui.colors.buttons[2]);
    let buffer = f.buffer_mut();

    for x_idx in (0..width).step_by(3) {
        let val = prev_lock[x_idx];
        let total_levels = (val * height as f32 * 8.0) as usize;
        let full_blocks = total_levels / 8;
        let partial_level = total_levels % 8;

        for y in 0..height {
            let cell_y = inner_area.bottom().saturating_sub(1 + y as u16);
            if cell_y < inner_area.top() { break; }

            let sym = if y < full_blocks { "█" }
                      else if y == full_blocks && partial_level > 0 { symbols[(partial_level - 1).min(6)] }
                      else if y == 0 { "▂" } // Наш фундамент
                      else { break; };

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