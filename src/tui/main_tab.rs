use crate::AudioEngine;
use crate::config::config::Config;
use crate::tui::widgets::{cava, library, logo, search, center};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Block, Borders},
};

pub fn draw_main_layout(f: &mut Frame, area: Rect, engine: &AudioEngine) {
    let conf = Config::global();

    // Если CAVA скрыт настройкой, локально отдаем 0 для расчета высот в Layout
    let current_cava_height = if conf.ui.cava_show {
        conf.ui.cava_height
    } else {
        0
    };

    // 1. Делим на верх (Search + Main) и низ (CAVA)
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(current_cava_height)])
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

    // Если CAVA скрыт, дорисовываем нижнюю линию рамок Main блока
    let main_borders = if conf.ui.cava_show {
        Borders::LEFT | Borders::RIGHT
    } else {
        Borders::LEFT | Borders::RIGHT | Borders::BOTTOM
    };

    f.render_widget(Block::default().borders(main_borders), stepped_area);

    // Панель CAVA рендерится только если она активна в конфиге
    if conf.ui.cava_show {
        f.render_widget(Block::default().borders(Borders::ALL), root[1]);
    }

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

    // Восстанавливаем углы нижней рамки Main блока, если CAVA скрыт
    if !conf.ui.cava_show {
        if let Some(cell) = buf.cell_mut((
            stepped_area.right().saturating_sub(1),
            stepped_area.bottom().saturating_sub(1),
        )) {
            cell.set_symbol("┘");
        }
        if let Some(cell) =
            buf.cell_mut((stepped_area.left(), stepped_area.bottom().saturating_sub(1)))
        {
            cell.set_symbol("└");
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
    search::draw_search_widget(f, top_parts[0]);
    library::draw_library_widget(f, top_parts[1]);
    logo::draw_rmpt_logo(f, top_parts[1]);
    center::draw_center_area(f, top_parts[0]);

    // Сам виджет спектрограммы рисуем только при show_cava = true
    if conf.ui.cava_show {
        let data_guard = engine.cava_data.try_lock();
        let freqs = match data_guard {
            Ok(ref data) => data,
            Err(_) => &vec![0.0; 128],
        };
        cava::draw_cava_widget(f, root[1], freqs);
    }
}
