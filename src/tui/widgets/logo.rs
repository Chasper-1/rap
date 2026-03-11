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
    let [r, g, b] = conf.ui.colors.logo; // Разбираем массив из конфига

    let logo = "█▀█
█▀▄

█▄█
█ █

█▀█
█▀ 

▀█▀
 █ ";

    let paragraph = Paragraph::new(Text::from(logo))
        .style(Style::default().fg(Color::Rgb(r, g, b)).bold());

    let area = Rect {
        x: conf.ui.logo_x,
        y: conf.ui.logo_y,
        width: 3,
        height: 11,
    };

    f.render_widget(paragraph, area);
}