use ratatui::{
    layout::{Rect, Margin},
    widgets::Paragraph,
    style::{Style, Color},
    Frame,
};
// Импортируем RngExt, чтобы заработал метод .random_range()
use rand::RngExt;

pub fn draw_cava_widget(f: &mut Frame, area: Rect) {
    if area.height < 3 {
        return;
    }

    let inner_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    
    let width = inner_area.width as usize;
    let height = inner_area.height as usize;

    // В 0.10 по-прежнему используем rng()
    let mut rng = rand::rng();
    
    let symbols = [" ", " ", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

    let mut cava_content = String::new();

    for h in 0..height {
        let mut line = String::with_capacity(width);
        for _ in 0..width {
            // Теперь это должно сработать
            let idx = rng.random_range(0..symbols.len());
            line.push_str(symbols[idx]);
        }
        cava_content.push_str(&line);
        if h < height - 1 {
            cava_content.push('\n');
        }
    }

    f.render_widget(
        Paragraph::new(cava_content).style(Style::default().fg(Color::Cyan)),
        inner_area
    );
}