use crate::config::config::Config;
use ratatui::Frame;
use ratatui::layout::Rect;

// Теперь функция принимает два аргумента, как ты и хочешь
pub fn draw_center_area(f: &mut Frame, _area: Rect) {
    let conf = Config::global();

    if conf.ui.center_width == 0 {
        return;
    }

    let x1 = conf.ui.library_x + conf.ui.library_width;
    let x2 = x1 + conf.ui.center_width;

    let buf = f.buffer_mut();

    for y in 1..(buf.area.height - 1) {
        if let Some(cell) = buf.cell_mut((x1, y)) {
            cell.set_symbol("│");
        }

        if let Some(cell) = buf.cell_mut((x2, y)) {
            cell.set_symbol("│");
        }
    }
}
