use ratatui::{
    Frame,
    layout::{Margin, Rect},
    style::{Color, Style},
    widgets::Paragraph,
};
use std::sync::{Mutex, OnceLock};

// Хранилище для инерции внутри самого виджета
static PREV_FREQS: OnceLock<Mutex<Vec<f32>>> = OnceLock::new();

pub fn draw_cava_widget(f: &mut Frame, area: Rect, raw_frequencies: &[f32]) {
    let conf = crate::config::config::Config::global();
    let ui = &conf.ui;

    if area.height < 2 || raw_frequencies.is_empty() { return; }

    let inner_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    let width = inner_area.width as usize;
    let height = inner_area.height as usize;

    let mut prev_lock = PREV_FREQS.get_or_init(|| Mutex::new(vec![0.0; 512])).lock().unwrap();
    if prev_lock.len() != raw_frequencies.len() {
        *prev_lock = vec![0.0; raw_frequencies.len()];
    }

    // Считаем среднюю энергию кадра для детектора тишины
    let avg_energy: f32 = raw_frequencies.iter().sum::<f32>() / raw_frequencies.len() as f32;

    let frequencies: Vec<f32> = raw_frequencies
        .iter()
        .enumerate()
        .map(|(i, &raw_val)| {
            let prev_val = prev_lock[i];

            // Если общая энергия кадра ниже порога — принудительно падаем в ноль
            if avg_energy < ui.cava_noise_gate {
                let dropped = (prev_val * ui.cava_fall_speed).max(0.0);
                prev_lock[i] = dropped;
                return dropped;
            }

            // ВЫРАВНИВАНИЕ ЯМЫ (Bell Curve)
            let pos = i as f32 / raw_frequencies.len() as f32;
            // Усиливаем центр (pos 0.5), игнорируя края (бас и вч)
            let bell = (-(pos - 0.5).powi(2) * 8.0).exp(); 
            let total_boost = 1.0 + (ui.cava_tilt * bell);

            // Расчет целевой высоты
            let target = (raw_val * ui.cava_sensitivity * total_boost).powf(ui.cava_exponent);

            // Плавность взлета и падения
            let final_val = if target > prev_val {
                prev_val + (target - prev_val) * ui.cava_attack
            } else {
                (prev_val * ui.cava_fall_speed).max(0.0)
            };

            let out = final_val.clamp(0.0, 1.0);
            prev_lock[i] = out;
            out
        })
        .collect();

    // Отрисовка
    let symbols = [" ", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let mut cava_content = String::with_capacity(width * height * 4);

    for h_idx in (0..height).rev() {
        let mut line = String::with_capacity(width * 4);
        let mut i = 0;
        while i < width {
            let start = (i * frequencies.len()) / width;
            let end = ((i + 1) * frequencies.len()) / width;
            let chunk = &frequencies[start..end.max(start + 1)];
            let val = chunk.iter().sum::<f32>() / chunk.len() as f32;

            let level_min = h_idx as f32 / height as f32;
            let level_max = (h_idx + 1) as f32 / height as f32;

            let char_idx = if val >= level_max { 7 }
            else if val > level_min {
                (((val - level_min) / (level_max - level_min)) * 8.0) as usize
            } else { 0 };

            let sym = symbols[char_idx.min(7)];
            line.push_str(sym);
            if i + 1 < width { line.push_str(sym); }
            if i + 2 < width { line.push(' '); }
            i += 3;
        }
        cava_content.push_str(&line);
        if h_idx > 0 { cava_content.push('\n'); }
    }

    f.render_widget(
        Paragraph::new(cava_content).style(Style::default().fg(Color::Rgb(ui.colors.buttons[0], ui.colors.buttons[1], ui.colors.buttons[2]))),
        inner_area,
    );
}