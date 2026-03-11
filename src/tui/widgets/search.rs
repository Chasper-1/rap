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

    // 1. ВЕРХНЯЯ ЛИНИЯ (y)
    draw_line_button(f, x + 2, y, "SEARCH", false); // Без скобок
    for i in 0..3 {
        let btn_x = x + w - (4 - i as u16) * 4;
        draw_line_button(f, btn_x, y, &(i + 1).to_string(), true); // Со скобками
    }

    // 3. СРЕДНЯЯ ЛИНИЯ (y + 2)
    for i in 0..w {
        let sym = if i == 0 { "├" } else { "─" };
        if let Some(cell) = f.buffer_mut().cell_mut((x + i, y + 2)) {
            cell.set_symbol(sym);
        }
    }
    draw_line_button(f, x + 2, y + 2, "REGEX", false); // Без скобок
    for i in 0..3 {
        let btn_x = x + w - (4 - i as u16) * 4;
        draw_line_button(f, btn_x, y + 2, &(i + 4).to_string(), true); // Со скобками
    }

    // 5. БОКОВУШКИ (Твой оригинальный код)
    for row in 0..h {
        let cur_y = y + row;
        if cur_y == y { continue; }

        if cur_y != y + 2 && cur_y != y + h.saturating_sub(1) {
            if let Some(cell) = f.buffer_mut().cell_mut((x, cur_y)) { cell.set_symbol("│"); }
            if let Some(cell) = f.buffer_mut().cell_mut((x + w - 1, cur_y)) { cell.set_symbol("│"); }
        } else if cur_y == y + 2 {
            if let Some(cell) = f.buffer_mut().cell_mut((x + w - 1, cur_y)) { cell.set_symbol("┤"); }
        }
    }
}

// Универсальная рисовалка: use_brackets рулит логикой
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