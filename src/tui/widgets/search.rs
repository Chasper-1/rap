use ratatui::{layout::Rect, Frame};
use crate::config::config::Config;

pub fn draw_search_widget(f: &mut Frame, _area: Rect) {
    let conf = Config::global();
    let x = conf.ui.search_box_x;
    let y = conf.ui.search_box_y;
    let w = conf.ui.search_box_width;
    let h = conf.ui.search_box_height;

    // --- 1. СТРОКА SEARCH (y) ---
    // Выводим только текст, без рамок
    for (i, ch) in "SEARCH".chars().enumerate() {
        if let Some(cell) = f.buffer_mut().cell_mut((x + 2 + i as u16, y)) { cell.set_symbol(&ch.to_string()); }
    }
    // Кнопки [1][2][3]
    let btns_top = [("[3]", 5), ("[2]", 9), ("[1]", 13)];
    for (txt, offset) in btns_top {
        for (i, ch) in txt.chars().enumerate() {
            if let Some(cell) = f.buffer_mut().cell_mut((x + w - offset + i as u16, y)) { cell.set_symbol(&ch.to_string()); }
        }
    }

    // --- 2. СТРОКА REGEX (y + 2) ---
    // Здесь рисуем перекладину, раз она отделяет ввод
    for i in 1..w-1 {
        if let Some(cell) = f.buffer_mut().cell_mut((x + i, y + 2)) { cell.set_symbol("─"); }
    }
    if let Some(cell) = f.buffer_mut().cell_mut((x, y + 2)) { cell.set_symbol("├"); }
    if let Some(cell) = f.buffer_mut().cell_mut((x + w - 1, y + 2)) { cell.set_symbol("┤"); }
    
    for (i, ch) in "REGEX".chars().enumerate() {
        if let Some(cell) = f.buffer_mut().cell_mut((x + 2 + i as u16, y + 2)) { cell.set_symbol(&ch.to_string()); }
    }
    // Кнопки [4][5][6]
    let btns_mid = [("[6]", 5), ("[5]", 9), ("[4]", 13)];
    for (txt, offset) in btns_mid {
        for (i, ch) in txt.chars().enumerate() {
            if let Some(cell) = f.buffer_mut().cell_mut((x + w - offset + i as u16, y + 2)) { cell.set_symbol(&ch.to_string()); }
        }
    }

    // --- 3. БОКОВЫЕ ЛИНИИ ---
    // Рисуем │ только по бокам, не заходя на верхнюю и нижнюю границы
    for row in 0..h {
        let cur_y = y + row;
        // Не рисуем углы, только палки │ там, где они не пересекаются с REGEX
        if cur_y != y + 2 {
            if let Some(cell) = f.buffer_mut().cell_mut((x, cur_y)) { cell.set_symbol("│"); }
            if let Some(cell) = f.buffer_mut().cell_mut((x + w - 1, cur_y)) { cell.set_symbol("│"); }
        }
    }
}