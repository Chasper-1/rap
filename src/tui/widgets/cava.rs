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
    let ui = &conf.ui; // Для краткости

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
        .map(|(i, &raw_val)| {
            let prev_val = prev_lock[i];

            // 1. УСИЛЕНИЕ И ВЫРАВНИВАНИЕ (БЕЗ ЖЕСТКОГО ЛОГАРИФМА)
            // Используем powf(0.5) - это корень. Он дает динамику, но не "плющит" звук как log10
            let mut val = raw_val.powf(0.5) * ui.cava_sensitivity;

            // 2. ВЕСА (EQ)
            // Низкие частоты обычно сильнее, поэтому мы их чуть придушим (0.8),
            // а высокие (индекс i растет) - подтянем.
            let pos = i as f32 / raw_frequencies.len() as f32;
            let weight = 0.8 + (pos * 1.5); // Линейно увеличиваем громкость к высоким
            val *= weight;

            // 3. АВТО-СБРОС (Главная фишка)
            // Если значение упало ниже предыдущего, мы СРАЗУ включаем падение.
            // Никакого "сглаживания взлета" (атаки), если хочешь резкости.
            let mut final_val = if val > prev_val {
                val // Резкий прыжок вверх (как в оригинале)
            } else {
                prev_val * ui.cava_fall_speed // Плавный спад
            };

            // 4. ОГРАНИЧЕНИЕ И "ПОЛ"
            // Не даем залипать в потолке
            if final_val > 1.0 {
                final_val = 1.0;
            }

            // Если сигнал совсем сдох, принудительно тянем к фундаменту
            if raw_val < ui.cava_noise_gate {
                final_val = (prev_val * ui.cava_fall_speed).max(0.05);
            }

            prev_lock[i] = final_val;
            final_val
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
