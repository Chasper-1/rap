use ratatui::{
    Frame,
    layout::{Margin, Rect},
    style::Color,
};

pub fn draw_cava_widget(f: &mut Frame, area: Rect, frequencies: &[f32]) {
    let conf = crate::config::config::Config::global();
    let ui = &conf.ui;

    if area.height < 2 || frequencies.is_empty() {
        return;
    }

    let inner_area = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let width = inner_area.width as usize;
    let height = inner_area.height as usize;

    let symbols = ["▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let main_color = Color::Rgb(
        ui.colors.buttons[0],
        ui.colors.buttons[1],
        ui.colors.buttons[2],
    );
    let buffer = f.buffer_mut();

    // Считаем, сколько всего столбиков (2 символа + 1 пробел) влезет в ширину
    let num_bars = width / 3;
    if num_bars == 0 {
        return;
    }

    for i in 0..num_bars {
        // РАСПРЕДЕЛЕНИЕ:
        // Масштабируем индекс так, чтобы i=0 брал frequencies[0],
        // а последний столбик i=(num_bars-1) брал самый конец массива frequencies.
        let freq_idx = (i * frequencies.len()) / num_bars;
        let val = frequencies.get(freq_idx).cloned().unwrap_or(0.0);

        // Позиция на экране
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
                symbols[(partial_level - 1).min(6)]
            } else if y == 0 {
                "▂"
            } else {
                break;
            };

            // Отрисовка "двойного" столбика
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
