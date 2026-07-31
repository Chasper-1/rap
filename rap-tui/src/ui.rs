use std::io::{self, Write};

use crossterm::cursor::MoveTo;
use crossterm::queue;
use crossterm::style::{Attribute, Print};
use crossterm::terminal::{Clear, ClearType};

use crate::i18n::Strings;
use crate::scanner::Entry;

/// Частичные блоки заполнения (1/8 .. 7/8 ширины символа): точный край
/// полоски. Пустая часть — пробелы, поэтому тёмный «хвост» частичного
/// блока сливается с фоном терминала и дыры не видно.
const PARTIAL_BLOCKS: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];

/// Полоска прогресса: границы '[ ]' схематично обозначают её место,
/// заполнение — '█', край — частичный блок 1/8 (▏..▉), пустая часть —
/// пробелы. Без цветов — видна в любой теме терминала. width — общая
/// ширина, включая скобки. progress — 0..1.
pub fn progress_bar(progress: f32, width: usize) -> String {
    if width < 2 {
        return String::new();
    }
    let inner = width - 2;
    // Ширина заполнения в восьмушках символа
    let eighths = (progress.clamp(0.0, 1.0) * inner as f32 * 8.0).round() as usize;
    let full = eighths / 8;
    let rem = eighths % 8;
    let mut s = String::with_capacity(width);
    s.push('[');
    for _ in 0..full {
        s.push('█');
    }
    if rem > 0 {
        s.push(PARTIAL_BLOCKS[rem - 1]);
    }
    let empty = inner.saturating_sub(full + usize::from(rem > 0));
    for _ in 0..empty {
        s.push(' ');
    }
    s.push(']');
    s
}

/// Обрезает строку до ширины (по символам, не байтам).
pub fn truncate(s: &str, width: usize) -> String {
    s.chars().take(width).collect()
}

/// Рисует весь экран: заголовок, список, строку текущего трека, полоску.
#[allow(clippy::too_many_arguments)]
pub fn render<W: Write>(
    out: &mut W,
    strings: &Strings,
    path: &str,
    entries: &[Entry],
    selected: usize,
    track_name: Option<&str>,
    paused: bool,
    progress: f32,
    volume: f32,
) -> io::Result<()> {
    let (w, h) = crossterm::terminal::size().unwrap_or((80, 24));
    let (w, h) = (w as usize, h as usize);
    let list_h = h.saturating_sub(3); // минус заголовок, строка трека и полоска

    // Заголовок
    queue!(
        out,
        MoveTo(0, 0),
        Print(truncate(&strings.header_with(path), w)),
        Clear(ClearType::UntilNewLine)
    )?;

    // Список
    for i in 0..list_h {
        queue!(out, MoveTo(0, i as u16 + 1))?;
        let line = match entries.get(i) {
            None if i == 0 => strings.empty_dir.clone(),
            None => String::new(),
            Some(e) if e.is_dir => format!("{}/", e.name),
            Some(e) => e.name.clone(),
        };
        let line = truncate(&line, w);
        if i == selected {
            queue!(
                out,
                Print(Attribute::Reverse),
                Print(&line),
                Print(Attribute::Reset),
                Clear(ClearType::UntilNewLine)
            )?;
        } else {
            queue!(out, Print(&line), Clear(ClearType::UntilNewLine))?;
        }
    }

    // Строка текущего трека
    let track_line = match track_name {
        Some(name) => {
            let suffix = if paused { strings.paused.as_str() } else { "" };
            format!(
                "{}{}{}  |  {}",
                strings.now_playing,
                name,
                suffix,
                strings.volume_with((volume * 100.0).round() as u32)
            )
        }
        None => strings.volume_with((volume * 100.0).round() as u32),
    };
    queue!(
        out,
        MoveTo(0, h.saturating_sub(2) as u16),
        Print(truncate(&track_line, w)),
        Clear(ClearType::UntilNewLine)
    )?;

    // Полоска прогресса
    queue!(
        out,
        MoveTo(0, h.saturating_sub(1) as u16),
        Print(progress_bar(progress, w))
    )?;

    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_zero() {
        assert_eq!(progress_bar(0.0, 10), "[        ]");
    }

    #[test]
    fn progress_full() {
        assert_eq!(progress_bar(1.0, 10), "[████████]");
    }

    #[test]
    fn progress_half_even() {
        assert_eq!(progress_bar(0.5, 10), "[████    ]");
    }

    #[test]
    fn progress_zero_width() {
        assert_eq!(progress_bar(0.5, 0), "");
        assert_eq!(progress_bar(0.5, 1), "");
        assert_eq!(progress_bar(0.5, 2), "[]");
    }

    #[test]
    fn progress_clamped() {
        assert_eq!(progress_bar(1.5, 5), "[███]");
        assert_eq!(progress_bar(-0.5, 5), "[   ]");
    }

    #[test]
    fn progress_rounding() {
        // 0.1 * 8 = 0.8 символа = 6.4/8 → округление 6 → '▊'
        assert_eq!(progress_bar(0.1, 10), "[▊       ]");
        // 0.15 * 8 = 1.2 символа = 9.6/8 → 10 восьмушек = 1 полный + 2/8 → '▎'
        assert_eq!(progress_bar(0.15, 10), "[█▎      ]");
        // 0.125 * 8 = 1 символ ровно
        assert_eq!(progress_bar(0.125, 10), "[█       ]");
        // 0.95 * 8 = 7.6 символа = 60.8/8 → 61 восьмушка = 7 полных + 5/8 → '▋'
        assert_eq!(progress_bar(0.95, 10), "[███████▋]");
    }

    #[test]
    fn truncate_by_chars() {
        assert_eq!(truncate("абвгде", 3), "абв");
        assert_eq!(truncate("short", 10), "short");
    }
}
