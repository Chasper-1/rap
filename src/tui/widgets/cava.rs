use ratatui::{
    layout::{Rect, Margin},
    widgets::Paragraph,
    style::{Style, Color},
    Frame,
};

pub fn draw_cava_widget(f: &mut Frame, area: Rect, frequencies: &[f32]) {
    if area.height < 3 || frequencies.is_empty() {
        return;
    }

    let inner_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    let width = inner_area.width as usize;
    let height = inner_area.height as usize;

    let symbols = [" ", " ", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let mut cava_content = String::new();

    // Нам нужно сопоставить количество данных FFT с шириной экрана (182 символа)
    // Если данных больше или меньше, делаем простой ресемплинг
    for h in (0..height).rev() { // Рисуем снизу вверх, если строк больше одной
        let mut line = String::with_capacity(width);
        
        for i in 0..width {
            // Берем значение частоты для этого столбика
            // Если данных меньше ширины, берем ближайший индекс
            let data_idx = (i * frequencies.len()) / width;
            let val = frequencies.get(data_idx).unwrap_or(&0.0);
            
            // Превращаем 0.0..1.0 в индекс символа
            let symbol_idx = ((*val * (symbols.len() - 1) as f32) as usize).min(symbols.len() - 1);
            line.push_str(symbols[symbol_idx]);
        }
        cava_content.push_str(&line);
        if h > 0 { cava_content.push('\n'); }
    }

    f.render_widget(
        Paragraph::new(cava_content).style(Style::default().fg(Color::Cyan)),
        inner_area
    );
}