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
    let (lang_arg, path_arg) = parse_args();
    let mut store = rap_store::Store::open();

    // Язык: аргумент --lang имеет приоритет, иначе — сохранённый в БД.
    let lang = match lang_arg {
        Some(l) => l,
        None => store.get_lang().await.unwrap_or_else(|| "ru".to_string()),
    };
    let _ = store.set_lang(&lang).await;

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

    let mut app = app::App::new(strings, root).await;
    app.run().await;
    store.shutdown();

    let _ = execute!(
        stdout,
        crossterm::event::DisableMouseCapture,
        LeaveAlternateScreen
    );
    let _ = disable_raw_mode();
}

/// Разбор аргументов: `rap-tui [путь-к-папке] [--lang код]`.
/// `--lang` вернётся как `Some`, если был указан.
fn parse_args() -> (Option<String>, Option<PathBuf>) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    parse_args_from(&args)
}

fn parse_args_from(args: &[String]) -> (Option<String>, Option<PathBuf>) {
    let mut lang = None;
    let mut path = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--lang" && i + 1 < args.len() {
            lang = Some(args[i + 1].clone());
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
        let (lang, path) = parse_args_from(&[]);
        assert!(lang.is_none());
        assert!(path.is_none());
    }

    #[test]
    fn parse_lang_and_path() {
        let args = vec![
            "--lang".to_string(),
            "ru".to_string(),
            "/tmp/music".to_string(),
        ];
        let (lang, path) = parse_args_from(&args);
        assert_eq!(lang, Some("ru".to_string()));
        assert_eq!(path, Some(PathBuf::from("/tmp/music")));
    }
}
