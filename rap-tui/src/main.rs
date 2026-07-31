mod app;
mod i18n;
mod input;
mod queue;
mod scanner;
mod ui;

use std::io;
use std::path::PathBuf;
use std::process::exit;

use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

#[rap_engine::tokio::main]
async fn main() {
    let (lang, path_arg) = parse_args();

    let strings = i18n::Strings::load(&lang);
    let root =
        path_arg.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    if !root.exists() {
        eprintln!(
            "{}",
            strings.error_bad_path_with(&root.display().to_string())
        );
        exit(1);
    }
    if !root.is_dir() {
        eprintln!("{}", strings.error_is_file);
        exit(1);
    }

    if enable_raw_mode().is_err() {
        eprintln!("Не удалось переключить терминал в raw mode");
        exit(1);
    }
    let mut stdout = io::stdout();
    // EnableMouseCapture перехватывает клики терминалом — текст не выделяется
    let _ = execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    );

    let mut app = app::App::new(strings, root);
    app.run().await;

    let _ = execute!(
        stdout,
        crossterm::event::DisableMouseCapture,
        LeaveAlternateScreen
    );
    let _ = disable_raw_mode();
}

/// Разбор аргументов: `rap-tui [путь-к-папке] [--lang код]`.
fn parse_args() -> (String, Option<PathBuf>) {
    let mut lang = "ru".to_string();
    let mut path = None;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--lang" && i + 1 < args.len() {
            lang = args[i + 1].clone();
            i += 2;
        } else {
            path = Some(PathBuf::from(&args[i]));
            i += 1;
        }
    }
    (lang, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_no_args() {
        let (lang, path) = parse_args();
        assert_eq!(lang, "ru");
        assert!(path.is_none());
    }
}
