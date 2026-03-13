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
    if prev_lock.len() != raw_frequencies.len() {
        *prev_lock = vec![0.0; raw_frequencies.len()];
    }

    let avg_energy: f32 = raw_frequencies.iter().sum::<f32>() / raw_frequencies.len() as f32;

    // --- ШАГ 1: ПЕРВИЧНАЯ ОБРАБОТКА И УСИЛЕНИЕ СЕРЕДИНЫ ---
    let mut target_freqs: Vec<f32> = raw_frequencies
        .iter()
        .enumerate()
        .map(|(i, &raw_val)| {
            let pos = i as f32 / raw_frequencies.len() as f32;

            // Оставляем Bell Curve для середины, но чуть мягче (8.0 -> 6.0)
            let bell = (-(pos - 0.5).powi(2) * 6.0).exp();
            let total_boost = 1.0 + (ui.cava_tilt * bell);

            // Основной сигнал с твоим гейном
            let mut val = raw_val * ui.cava_sensitivity * total_boost;

            // --- МАГИЯ: Тангенциальное сжатие (Soft Clip) ---
            // Это позволяет графику летать, но не дает ему биться головой об потолок.
            // Вместо жесткого clamp(0.0, 1.0), мы используем гиперболический тангенс.
            val = (val * 1.5).tanh();

            val.powf(ui.cava_exponent).clamp(0.0, 1.0)
        })
        .collect();

    // --- ШАГ 2: ЧАСТОТНОЕ СГЛАЖИВАНИЕ (Убирает "дёрганые" палки) ---
    // Проходимся 2 раза для мягкости
    for _ in 0..2 {
        for i in 1..target_freqs.len() - 1 {
            target_freqs[i] =
                target_freqs[i - 1] * 0.25 + target_freqs[i] * 0.5 + target_freqs[i + 1] * 0.25;
        }
    }

    // --- ШАГ 3: ВРЕМЕННАЯ ПЛАВНОСТЬ (Атака и Гравитация) ---
    for i in 0..target_freqs.len() {
        let target = target_freqs[i];
        let prev = prev_lock[i];

        if avg_energy < ui.cava_noise_gate {
            // При тишине падаем быстрее
            prev_lock[i] = (prev * 0.8).max(0.0);
        } else if target > prev {
            // Взлет: используем атаку (плавный подъем)
            prev_lock[i] = prev + (target - prev) * ui.cava_attack;
        } else {
            // Падение: имитируем инерцию (нелинейное падение)
            // Чем выше был столбик, тем быстрее он начинает падать
            let fall = (prev - target) * (1.0 - ui.cava_fall_speed);
            prev_lock[i] = (prev - fall).max(0.0);
        }
    }

    // --- ШАГ 4: ОТРИСОВКА ---
    let symbols = ["▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let main_color = Color::Rgb(
        ui.colors.buttons[0],
        ui.colors.buttons[1],
        ui.colors.buttons[2],
    );
    let buffer = f.buffer_mut();

    for x_idx in (0..width).step_by(3) {
        let freq_idx = (x_idx * target_freqs.len()) / width;
        let val = prev_lock[freq_idx];

        // 8 уровней символов на ячейку высоты
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
                "▂" // Вечный фундамент
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
