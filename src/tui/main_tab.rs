use crate::config::config::Config;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
    style::Style,
};
use crate::tui::widgets::{search, library, logo};

// Теперь передаем сюда состояние, которое Tokio обновляет в фоне
// (AppState создадим позже, пока просто заложим логику)
pub fn draw_main_layout(f: &mut Frame, area: Rect) {
    let conf = Config::global();

    // 1. Делим на верх (Search + Main) и низ (CAVA)
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0), 
            Constraint::Length(conf.ui.cava_height)
        ])
        .split(area);

    let top_parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(conf.ui.search_height),
            Constraint::Min(0),
        ])
        .split(root[0]);

    // --- РИСУЕМ БЛОКИ ---
    
    // Search
    f.render_widget(
        Block::default().borders(Borders::TOP | Borders::LEFT | Borders::RIGHT),
        top_parts[0],
    );

    // Main с твоей ступенькой
    let stepped_area = Rect {
        x: area.x + conf.ui.step_offset,
        y: top_parts[1].y,
        width: area.width.saturating_sub(conf.ui.step_offset),
        height: top_parts[1].height,
    };
    
    f.render_widget(
        Block::default().borders(Borders::LEFT | Borders::RIGHT),
        stepped_area,
    );

    // CAVA
    f.render_widget(Block::default().borders(Borders::ALL), root[1]);

    // --- МАГИЯ СТЫКА ---
    let buf = f.buffer_mut();
    let x_left_search = top_parts[0].x;
    let x_left_main = stepped_area.x;
    let y_join = top_parts[1].y;

    if conf.ui.step_offset > 0 {
        let line = "─".repeat(conf.ui.step_offset as usize);
        buf.set_string(x_left_search + 1, y_join, &line, Style::default());

        if let Some(cell) = buf.cell_mut((x_left_search, y_join)) {
            cell.set_symbol("└");
        }
        if let Some(cell) = buf.cell_mut((x_left_main, y_join)) {
            cell.set_symbol("┐");
        }
    }

    // --- ЛИНИЯ ИЗ КОНФИГА ---
    if conf.ui.line_width > 0 {
        buf.set_string(
            conf.ui.line_x,
            conf.ui.line_y,
            "─".repeat(conf.ui.line_width as usize),
            Style::default(),
        );
    }
    
    // --- ВЫЗОВ ВИДЖЕТОВ ---
    // В Tokio-версии эти функции должны быстро забирать данные из кэша
    search::draw_search_widget(f, top_parts[0]);
    library::draw_library_widget(f, top_parts[1]); 
    logo::draw_rmpt_logo(f, top_parts[1]);
}