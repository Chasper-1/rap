use ratatui::{
    Frame,
    layout::{Margin, Rect},
    style::Color,
};

pub fn draw_cava_widget(f: &mut Frame, area: Rect, frequencies: &[f32]) {
    let conf = crate::config::config::Config::global();
    let ui = &conf.ui;

    if area.height < 2 { return; }

    let inner_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    let width = inner_area.width as usize;
    let height = inner_area.height as usize;

    // Отрисовка
    let symbols = ["▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let main_color = Color::Rgb(ui.colors.buttons[0], ui.colors.buttons[1], ui.colors.buttons[2]);
    let buffer = f.buffer_mut();

    // Мы рисуем столбики с шагом 3 (2 символа столбца + 1 пробел)
    for x in (0..width).step_by(3) {
        // Берем значение напрямую, так как анализатор уже подготовил данные под эту ширину
        let val = frequencies.get(x).cloned().unwrap_or(0.0);
        
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

            for offset in 0..2 {
                let cell_x = inner_area.left() + (x + offset) as u16;
                if cell_x < inner_area.right() {
                    if let Some(cell) = buffer.cell_mut((cell_x, cell_y)) {
                        cell.set_symbol(sym).set_fg(main_color);
                    }
                }
            }
        }
    }
}