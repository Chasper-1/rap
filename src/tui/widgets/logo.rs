use ratatui::{
    style::{Color, Style},
    layout::Rect,
    text::Text,
    widgets::Paragraph,
    Frame,
};
use crate::config::config::Config;

pub fn draw_rmpt_logo(f: &mut Frame, _area: Rect) {
    let conf = Config::global();

    // Логотип остается как есть, ровно в 3 колонки
    let logo = "█▀█
█▀▄

█▄█
█ █

█▀█
█▀ 

▀█▀
 █ ";

    let paragraph = Paragraph::new(Text::from(logo))
        .style(Style::default().fg(Color::Rgb(167, 192, 128)).bold());

    // Задаем жесткий Rect. Никаких f.size() и динамики
    // Это выключает лишние пересчеты и «растягивание» на всю строку
    let area = Rect {
        x: conf.ui.logo_x,
        y: conf.ui.logo_y,
        width: 3,   // Жестко 3 символа
        height: 11, // Жестко под высоту текста
    };

    f.render_widget(paragraph, area);
}