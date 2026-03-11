use ratatui::{
    layout::Rect,
    Frame,
    style::{Color, Style},
};
use crate::config::config::Config;

pub fn draw_library_widget(f: &mut Frame, _area: Rect) {
    let conf = &Config::global().ui; // Обращаемся к UI напрямую

    let x = conf.library_x;
    let y = conf.library_y;
    let w = conf.library_width;
    let h = conf.library_height;

    if w < 2 || h < 2 { return; }

    let [lr, lg, lb] = conf.colors.library_label;
    let [br, bg, bb] = conf.colors.buttons;
    let lib_style = Style::default().fg(Color::Rgb(lr, lg, lb));
    let btn_style = Style::default().fg(Color::Rgb(br, bg, bb));

    // Рамки
    for i in 0..w {
        if let Some(cell) = f.buffer_mut().cell_mut((x + i, y)) { cell.set_symbol("─"); }
        if let Some(cell) = f.buffer_mut().cell_mut((x + i, y + h - 1)) { cell.set_symbol("─"); }
    }
    for row in 0..h {
        if let Some(cell) = f.buffer_mut().cell_mut((x, y + row)) { cell.set_symbol("│"); }
        if let Some(cell) = f.buffer_mut().cell_mut((x + w - 1, y + row)) { cell.set_symbol("│"); }
    }
    // Углы
    if let Some(cell) = f.buffer_mut().cell_mut((x, y)) { cell.set_symbol("┌"); }
    if let Some(cell) = f.buffer_mut().cell_mut((x + w - 1, y)) { cell.set_symbol("┐"); }
    if let Some(cell) = f.buffer_mut().cell_mut((x, y + h - 1)) { cell.set_symbol("└"); }
    if let Some(cell) = f.buffer_mut().cell_mut((x + w - 1, y + h - 1)) { cell.set_symbol("┘"); }

    // Заголовок
    let title = "LIBRARY";
    for (i, ch) in title.chars().enumerate() {
        if let Some(cell) = f.buffer_mut().cell_mut((x + 33 + i as u16, y)) {
            cell.set_symbol(&ch.to_string()).set_style(lib_style);
        }
    }

    // Кнопки
    let buttons = ["[1]", "[2]", "[3]", "[4]"];
    for (i, btn) in buttons.iter().enumerate() {
        for (ch_idx, ch) in btn.chars().enumerate() {
            if let Some(cell) = f.buffer_mut().cell_mut((x + w - 2 + ch_idx as u16, y + 2 + (i as u16 * 2))) {
                cell.set_symbol(&ch.to_string()).set_style(btn_style);
            }
        }
    }
}