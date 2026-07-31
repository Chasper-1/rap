use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEventKind, MouseEventKind};
use rap_engine::AudioEngine;
use rap_engine::probe::probe_duration;
use rap_engine::tokio::select;
use rap_engine::tokio::time::sleep;

use crate::i18n::Strings;
use crate::queue;
use crate::scanner::{self, Entry};
use crate::ui;

/// Интервал тика перерисовки/проверки конца трека.
const TICK: Duration = Duration::from_millis(150);
/// Сколько ждём после запуска трека, прежде чем считать его законченным.
const START_GRACE: Duration = Duration::from_secs(2);
/// Порог двойного клика мыши.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

struct Track {
    path: PathBuf,
    duration: Option<Duration>,
    index: usize,
    started: Instant,
}

pub struct App {
    strings: Strings,
    engine: AudioEngine,
    stack: Vec<PathBuf>,
    entries: Vec<Entry>,
    audio_files: Vec<PathBuf>,
    selected: usize,
    track: Option<Track>,
    paused: bool,
    running: bool,
    last_click: Option<(usize, Instant)>,
}

impl App {
    pub fn new(strings: Strings, root: PathBuf) -> Self {
        let entries = scanner::scan_dir(&root).unwrap_or_default();
        let audio_files = scanner::audio_list(&entries);
        Self {
            strings,
            engine: AudioEngine::new(),
            stack: vec![root],
            entries,
            audio_files,
            selected: 0,
            track: None,
            paused: false,
            running: true,
            last_click: None,
        }
    }

    pub async fn run(&mut self) {
        let mut rx = crate::input::spawn_event_reader();
        while self.running {
            let tick = sleep(TICK);
            let event = select! {
                _ = tick => None,
                ev = rx.recv() => ev,
            };

            if let Some(event) = event {
                self.handle(event).await;
            }
            self.on_tick().await;
            let _ = self.render();
        }
        self.engine.shutdown().await;
    }

    async fn handle(&mut self, event: Event) {
        match event {
            Event::Key(k) if k.kind == KeyEventKind::Press => self.on_key(k.code).await,
            Event::Mouse(m) => self.on_mouse(m.kind, m.row).await,
            _ => {}
        }
    }

    async fn on_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                if self.selected + 1 < self.entries.len() {
                    self.selected += 1;
                }
            }
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = self.entries.len().saturating_sub(1),
            KeyCode::Enter => self.activate_selected().await,
            KeyCode::Backspace | KeyCode::Left => self.go_up(),
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char(' ') => self.toggle_pause().await,
            _ => {}
        }
    }

    async fn on_mouse(&mut self, kind: MouseEventKind, row: u16) {
        match kind {
            MouseEventKind::ScrollUp => self.selected = self.selected.saturating_sub(1),
            MouseEventKind::ScrollDown => {
                if self.selected + 1 < self.entries.len() {
                    self.selected += 1;
                }
            }
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                if row == 0 {
                    return;
                }
                let idx = (row - 1) as usize;
                if idx >= self.entries.len() {
                    return;
                }
                let now = Instant::now();
                let is_double =
                    matches!(self.last_click, Some((i, t)) if i == idx && now - t < DOUBLE_CLICK);
                self.last_click = Some((idx, now));
                self.selected = idx;
                if is_double {
                    self.activate_selected().await;
                }
            }
            _ => {}
        }
    }

    async fn activate_selected(&mut self) {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return;
        };
        if entry.is_dir {
            self.stack.push(entry.path);
            self.rescan();
        } else if let Some(idx) = self.audio_files.iter().position(|p| *p == entry.path) {
            self.play_at(idx).await;
        }
    }

    fn go_up(&mut self) {
        if self.stack.len() <= 1 {
            self.running = false;
            return;
        }
        self.stack.pop();
        self.rescan();
    }

    fn rescan(&mut self) {
        let dir = self.stack.last().expect("stack не пуст");
        self.entries = scanner::scan_dir(dir).unwrap_or_default();
        self.audio_files = scanner::audio_list(&self.entries);
        self.selected = 0;
    }

    async fn toggle_pause(&mut self) {
        if self.track.is_none() {
            return;
        }
        if self.paused {
            self.engine.resume().await;
        } else {
            self.engine.pause().await;
        }
        self.paused = !self.paused;
    }

    async fn play_at(&mut self, idx: usize) {
        let Some(path) = self.audio_files.get(idx).cloned() else {
            return;
        };
        let path_str = path.to_string_lossy().into_owned();
        self.engine.play(&path_str).await;

        let path2 = path.clone();
        let duration = select! {
            d = rap_engine::tokio::task::spawn_blocking(move || probe_duration(&path2)) => d.ok().flatten(),
        };
        self.track = Some(Track {
            path,
            duration,
            index: idx,
            started: Instant::now(),
        });
        self.paused = false;
        self.select_entry_of_current();
    }

    /// Авто-переход на следующий файл после окончания текущего.
    async fn on_tick(&mut self) {
        let Some(track) = self.track.as_ref() else {
            return;
        };
        if !self.engine.is_empty() {
            return;
        }
        // Движок может быть "пустым" первые мгновения после play, пока грузится файл
        if track.started.elapsed() < START_GRACE {
            return;
        }
        match queue::next_index(self.audio_files.len(), track.index) {
            Some(next) => self.play_at(next).await,
            None => {
                self.track = None;
                self.paused = false;
            }
        }
    }

    /// Подсвечивает в списке файл, который сейчас играет.
    fn select_entry_of_current(&mut self) {
        let Some(track) = &self.track else {
            return;
        };
        if let Some(i) = self.entries.iter().position(|e| e.path == track.path) {
            self.selected = i;
        }
    }

    fn render(&mut self) -> io::Result<()> {
        let stdout = io::stdout();
        let mut out = stdout.lock();

        let path = self
            .stack
            .last()
            .expect("stack не пуст")
            .display()
            .to_string();
        let (track_name, paused, progress) = match &self.track {
            Some(t) => {
                let name = t.path.file_name().map(|n| n.to_string_lossy().into_owned());
                let progress = match t.duration {
                    Some(dur) if !dur.is_zero() => {
                        self.engine.get_current_pos() as f32 / dur.as_secs_f32()
                    }
                    _ => 0.0,
                };
                (name, self.paused, progress)
            }
            None => (None, false, 0.0),
        };

        ui::render(
            &mut out,
            &self.strings,
            &path,
            &self.entries,
            self.selected,
            track_name.as_deref(),
            paused,
            progress,
        )
    }
}
