use std::io::{self, Write};

use crossterm::cursor::MoveTo;
use crossterm::queue;
use crossterm::style::{Attribute, Print};
use crossterm::terminal::{Clear, ClearType};

use crate::i18n::Strings;
use crate::scanner::Entry;

/// Полоска прогресса: заполненная часть — '█', пустая — '—'.
/// Без цветов — видна в любой теме терминала. progress — 0..1.
pub fn progress_bar(progress: f32, width: usize) -> String {
    let filled = (progress.clamp(0.0, 1.0) * width as f32).round() as usize;
    let mut s = String::with_capacity(width);
    for _ in 0..filled {
        s.push('█');
    }
    for _ in filled..width {
        s.push('—');
    }
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
            format!("{}{}{}", strings.now_playing, name, suffix)
        }
        None => String::new(),
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
        assert_eq!(progress_bar(0.0, 10), "——————————");
    }

    #[test]
    fn progress_full() {
        assert_eq!(progress_bar(1.0, 10), "██████████");
    }

    #[test]
    fn progress_half_even() {
        assert_eq!(progress_bar(0.5, 10), "█████—————");
    }

    #[test]
    fn progress_zero_width() {
        assert_eq!(progress_bar(0.5, 0), "");
    }

    #[test]
    fn progress_clamped() {
        assert_eq!(progress_bar(1.5, 5), "█████");
        assert_eq!(progress_bar(-0.5, 5), "—————");
    }

    #[test]
    fn truncate_by_chars() {
        assert_eq!(truncate("абвгде", 3), "абв");
        assert_eq!(truncate("short", 10), "short");
    }
}
