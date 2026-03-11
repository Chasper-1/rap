use ratatui::{
    layout::Rect,
    Frame,
};
use crate::config::config::Config;

pub fn draw_search_widget(f: &mut Frame, _area: Rect) {
    let conf = Config::global();
    let x = conf.ui.search_box_x;
    let y = conf.ui.search_box_y;
    let w = conf.ui.search_box_width;
    let h = conf.ui.search_box_height;

    // 1. ВЕРХНЯЯ СТРОКА (y) — ТОЛЬКО ТЕКСТ И КНОПКИ В КОНЦЕ
    draw_line_button(f, x + 2, y, "SEARCH", false); 
    
    // Кнопки [1], [2], [3] прижаты вправо (отступ 1 от края)
    draw_line_button(f, x + w - 13, y, "1", true);
    draw_line_button(f, x + w - 9,  y, "2", true);
    draw_line_button(f, x + w - 5,  y, "3", true);

    // 2. СРЕДНЯЯ ЛИНИЯ (y + 2) — ПЕРЕКЛАДИНА С КНОПКАМИ
    for i in 0..w {
        let sym = if i == 0 { "├" } else if i == w - 1 { "┤" } else { "─" };
        if let Some(cell) = f.buffer_mut().cell_mut((x + i, y + 2)) {
            cell.set_symbol(sym);
        }
    }
    draw_line_button(f, x + 2, y + 2, "REGEX", false);
    
    // Кнопки [4], [5], [6] прижаты вправо
    draw_line_button(f, x + w - 13, y + 2, "4", true);
    draw_line_button(f, x + w - 9,  y + 2, "5", true);
    draw_line_button(f, x + w - 5,  y + 2, "6", true);

    // 3. БОКОВУШКИ (БЕЗ ВЕРХА И НИЗА)
    for row in 0..h {
        let cur_y = y + row;
        
        // Убираем всё на самой верхней и самой нижней строках
        if cur_y == y || cur_y == y + h.saturating_sub(1) { continue; }

        // Рисуем │ только если это не линия REGEX
        if cur_y != y + 2 {
            if let Some(cell) = f.buffer_mut().cell_mut((x, cur_y)) { cell.set_symbol("│"); }
            if let Some(cell) = f.buffer_mut().cell_mut((x + w - 1, cur_y)) { cell.set_symbol("│"); }
        }
    }
}

// Твоя функция-рисовалка
fn draw_line_button(f: &mut Frame, x: u16, y: u16, label: &str, use_brackets: bool) {
    let buf = f.buffer_mut();
    let mut offset = 0;

    if use_brackets {
        if let Some(cell) = buf.cell_mut((x, y)) { cell.set_symbol("["); }
        offset = 1;
    }

    for (i, ch) in label.chars().enumerate() {
        if let Some(cell) = buf.cell_mut((x + offset + i as u16, y)) {
            cell.set_symbol(&ch.to_string());
        }
    }

    if use_brackets {
        if let Some(cell) = buf.cell_mut((x + offset + label.len() as u16, y)) {
            cell.set_symbol("]");
        }
    }
}