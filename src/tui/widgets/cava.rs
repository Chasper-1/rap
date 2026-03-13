use ratatui::{
    layout::{Rect, Margin},
    widgets::Paragraph,
    style::{Style, Color},
    Frame,
};

pub fn draw_cava_widget(f: &mut Frame, area: Rect, frequencies: &[f32]) {
    if area.height < 3 {
        return;
    }

    let inner_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    let width = inner_area.width as usize;
    let height = inner_area.height as usize;

    let symbols = [" ", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let mut cava_content = String::new();

    // Генерируем строки сверху вниз
    for h_idx in (0..height).rev() {
        let mut line = String::with_capacity(width);
        
        for i in 0..width {
            // Если массив частот пустой, рисуем "тишину" (самый первый символ)
            let val = if frequencies.is_empty() {
                if h_idx == 0 { 0.05 } else { 0.0 } // Маленькая полоска только на нижней строке
            } else {
                let data_idx = (i * frequencies.len()) / width;
                *frequencies.get(data_idx).unwrap_or(&0.0)
            };

            // Вычисляем, какой символ рисовать на текущей высоте
            // level — это относительная высота текущей строки (от 0.0 до 1.0)
            let level_min = h_idx as f32 / height as f32;
            let level_max = (h_idx + 1) as f32 / height as f32;

            let char_idx = if val >= level_max {
                8 // Полный блок
            } else if val > level_min {
                // Плавный переход внутри одного блока
                let internal_factor = (val - level_min) / (level_max - level_min);
                (internal_factor * 8.0) as usize
            } else {
                0 // Пусто
            };

            line.push_str(symbols[char_idx]);
        }
        cava_content.push_str(&line);
        if h_idx > 0 {
            cava_content.push('\n');
        }
    }

    f.render_widget(
        Paragraph::new(cava_content).style(Style::default().fg(Color::Cyan)),
        inner_area
    );
}