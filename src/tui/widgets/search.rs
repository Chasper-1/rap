use ratatui::{layout::Rect, Frame, style::{Color, Style}};
use crate::config::config::Config;

pub fn draw_search_widget(f: &mut Frame, _area: Rect) {
    let conf = &Config::global().ui;
    let x = conf.search_box_x;
    let y = conf.search_box_y;
    let w = conf.search_box_width;
    let h = conf.search_box_height;

    let [lr, lg, lb] = conf.colors.search_label;
    let [rr, rg, rb] = conf.colors.regex_label;
    let [br, bg, bb] = conf.colors.buttons;
    
    let search_style = Style::default().fg(Color::Rgb(lr, lg, lb));
    let regex_style = Style::default().fg(Color::Rgb(rr, rg, rb));
    let btn_style = Style::default().fg(Color::Rgb(br, bg, bb));

    draw_line_button(f, x + 2, y, "SEARCH", false, search_style); 
    draw_line_button(f, x + w - 13, y, "1", true, btn_style);
    draw_line_button(f, x + w - 9,  y, "2", true, btn_style);
    draw_line_button(f, x + w - 5,  y, "3", true, btn_style);

    for i in 0..w {
        let sym = if i == 0 { "├" } else if i == w - 1 { "┤" } else { "─" };
        if let Some(cell) = f.buffer_mut().cell_mut((x + i, y + 2)) {
            cell.set_symbol(sym);
        }
    }
    
    draw_line_button(f, x + 2, y + 2, "REGEX", false, regex_style);
    draw_line_button(f, x + w - 13, y + 2, "4", true, btn_style);
    draw_line_button(f, x + w - 9,  y + 2, "5", true, btn_style);
    draw_line_button(f, x + w - 5,  y + 2, "6", true, btn_style);

    for row in 0..h {
        let cur_y = y + row;
        if cur_y == y || cur_y == y + h.saturating_sub(1) { continue; }
        if cur_y != y + 2 {
            if let Some(cell) = f.buffer_mut().cell_mut((x, cur_y)) { cell.set_symbol("│"); }
            if let Some(cell) = f.buffer_mut().cell_mut((x + w - 1, cur_y)) { cell.set_symbol("│"); }
        }
    }
}

fn draw_line_button(f: &mut Frame, x: u16, y: u16, label: &str, use_brackets: bool, style: Style) {
    let buf = f.buffer_mut();
    let mut offset = 0;
    if use_brackets {
        if let Some(cell) = buf.cell_mut((x, y)) { cell.set_symbol("["); }
        offset = 1;
    }
    for (i, ch) in label.chars().enumerate() {
        if let Some(cell) = buf.cell_mut((x + offset + i as u16, y)) {
            cell.set_symbol(&ch.to_string()).set_style(style);
        }
    }
    if use_brackets {
        if let Some(cell) = buf.cell_mut((x + offset + label.len() as u16, y)) {
            cell.set_symbol("]");
        }
    }
}