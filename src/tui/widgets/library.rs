use ratatui::{layout::Rect, Frame};
use crate::config::config::Config;

pub fn draw_library_widget(f: &mut Frame, _area: Rect) {
    // Тот самый вызов, который "нюхает" твой конфиг из OnceLock
    let conf = &Config::global().ui; 
    
    let x = conf.library_x;
    let y = conf.library_y;
    let w = conf.library_width;
    let h = conf.library_height;

    // Если конфиг — говно и ширина нулевая, не рисуем
    if w < 2 || h < 2 { return; }

    // РИСУЕМ РАМКИ
    // Верхняя и нижняя линии
    for i in 0..w {
        if let Some(cell) = f.buffer_mut().cell_mut((x + i, y)) { cell.set_symbol("─"); }
        if let Some(cell) = f.buffer_mut().cell_mut((x + i, y + h - 1)) { cell.set_symbol("─"); }
    }
    // Боковые линии
    for row in 0..h {
        if let Some(cell) = f.buffer_mut().cell_mut((x, y + row)) { cell.set_symbol("│"); }
        if let Some(cell) = f.buffer_mut().cell_mut((x + w - 1, y + row)) { cell.set_symbol("│"); }
    }
    // Углы (красиво стыкуем)
    if let Some(cell) = f.buffer_mut().cell_mut((x, y)) { cell.set_symbol("┌"); }
    if let Some(cell) = f.buffer_mut().cell_mut((x + w - 1, y)) { cell.set_symbol("┐"); }
    if let Some(cell) = f.buffer_mut().cell_mut((x, y + h - 1)) { cell.set_symbol("└"); }
    if let Some(cell) = f.buffer_mut().cell_mut((x + w - 1, y + h - 1)) { cell.set_symbol("┘"); }

    // Заголовок прямо на рамке
    let title = "LIBRARY";
    for (i, ch) in title.chars().enumerate() {
        if let Some(cell) = f.buffer_mut().cell_mut((x + 2 + i as u16, y)) {
            cell.set_symbol(&ch.to_string());
        }
    }
}