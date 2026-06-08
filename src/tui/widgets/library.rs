use crate::config::config::Config;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
};

pub fn draw_library_widget(f: &mut Frame, _area: Rect) {
    let conf = &Config::global().ui;

    let x = conf.library_x;
    let y = conf.library_y;
    let w = conf.library_width;
    let h = conf.library_height;

    if w < 2 || h < 2 {
        return;
    }

    let [lr, lg, lb] = conf.colors.library_label;
    let [br, bg, bb] = conf.colors.buttons;
    let lib_style = Style::default().fg(Color::Rgb(lr, lg, lb));
    let btn_style = Style::default().fg(Color::Rgb(br, bg, bb));

    // Верхняя рамка
    for i in 0..w {
        if let Some(cell) = f.buffer_mut().cell_mut((x + i, y)) {
            cell.set_symbol("─");
        }
    }

    // Правая вертикальная рамка
    for row in 0..h {
        if let Some(cell) = f.buffer_mut().cell_mut((x + w - 1, y + row)) {
            cell.set_symbol("│");
        }
    }

    // Только верхний правый угол
    if let Some(cell) = f.buffer_mut().cell_mut((x + w - 1, y)) {
        cell.set_symbol("┐");
    }

    // Заголовок "LIBRARY"
    let title = "LIBRARY";
    for (i, ch) in title.chars().enumerate() {
        if let Some(cell) = f.buffer_mut().cell_mut((x + 33 + i as u16, y)) {
            cell.set_symbol(&ch.to_string()).set_style(lib_style);
        }
    }

    // Кнопки [1], [2]...
    let buttons = ["1", "2", "3", "4"];
    for (i, btn) in buttons.iter().enumerate() {
        for (ch_idx, ch) in btn.chars().enumerate() {
            if let Some(cell) = f
                .buffer_mut()
                .cell_mut((x + w - 1 + ch_idx as u16, y + 2 + (i as u16 * 2)))
            {
                cell.set_symbol(&ch.to_string()).set_style(btn_style);
            }
        }
    }
}