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

    if area.height < 2 || raw_frequencies.is_empty() { return; }

    let inner_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    let width = inner_area.width as usize;
    let height = inner_area.height as usize;

    let mut prev_lock = PREV_FREQS
        .get_or_init(|| Mutex::new(vec![0.0f32; 1024]))
        .lock()
        .unwrap();

    // Защита от изменения размера окна
    if prev_lock.len() < width { 
        prev_lock.resize(width + 10, 0.0); 
    }

    // 1. ПРЯМОЕ ОТОБРАЖЕНИЕ
    // Анализатор уже прислал нам 128 чистых столбиков. 
    // Нам нужно просто распределить их по ширине виджета.
    let mut target_freqs = vec![0.0f32; width];
    
    for x in (0..width).step_by(3) {
        // Пропорционально берем индекс из пришедших 128 частот
        let idx = (x * raw_frequencies.len()) / width;
        if let Some(&val) = raw_frequencies.get(idx) {
            target_freqs[x] = val.clamp(0.0, 1.0);
        }
    }

    // 2. ФИЗИКА (fall_speed и attack)
    // Оставляем её здесь, чтобы анимация была плавной именно на частоте кадров интерфейса
    for i in 0..width {
        let target = target_freqs[i];
        let prev = prev_lock[i];

        if target > prev {
            prev_lock[i] = prev + (target - prev) * ui.cava_attack;
        } else {
            prev_lock[i] = (prev * ui.cava_fall_speed).max(target);
        }
        
        if prev_lock[i] < 0.001 { prev_lock[i] = 0.0; }
    }

    // 3. ОТРИСОВКА
    let symbols = ["▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let main_color = Color::Rgb(ui.colors.buttons[0], ui.colors.buttons[1], ui.colors.buttons[2]);
    let buffer = f.buffer_mut();

    for x_idx in (0..width).step_by(3) {
        let val = prev_lock[x_idx];
        
        // Масштабируем значение под высоту виджета
        let total_levels = (val * height as f32 * 8.0) as usize;
        let full_blocks = total_levels / 8;
        let partial_level = total_levels % 8;

        for y in 0..height {
            let cell_y = inner_area.bottom().saturating_sub(1 + y as u16);
            if cell_y < inner_area.top() { break; }

            let sym = if y < full_blocks { "█" }
                      else if y == full_blocks && partial_level > 0 { symbols[(partial_level - 1).min(6)] }
                      else if y == 0 { "▂" } 
                      else { break; };

            // Рисуем столбик шириной в 2 символа
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