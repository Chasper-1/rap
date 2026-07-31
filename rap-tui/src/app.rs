use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEventKind, MouseEventKind};
use rap_engine::AudioEngine;
use rap_engine::probe::probe_duration;
use rap_engine::tokio::select;
use rap_engine::tokio::time::sleep;
use rap_store::Store;

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
/// Шаг изменения громкости.
const VOLUME_STEP: f32 = 0.05;
/// Шаг перемотки, секунд.
const SEEK_STEP: i64 = 5;

struct Track {
    path: PathBuf,
    duration: Option<Duration>,
    index: usize,
    started: Instant,
}

pub struct App {
    strings: Strings,
    engine: AudioEngine,
    store: Store,
    stack: Vec<PathBuf>,
    entries: Vec<Entry>,
    audio_files: Vec<PathBuf>,
    selected: usize,
    track: Option<Track>,
    paused: bool,
    volume: f32,
    running: bool,
    last_click: Option<(usize, Instant)>,
}

impl App {
    pub async fn new(strings: Strings, root: PathBuf) -> Self {
        let entries = scanner::scan_dir(&root).unwrap_or_default();
        let audio_files = scanner::audio_list(&entries);
        let store = Store::open();
        let volume = store.get_volume().await.unwrap_or(1.0);
        let engine = AudioEngine::new();
        engine.set_volume(volume).await;

        // Продолжение с места, где остановились в прошлый раз.
        // Трек запускается СРАЗУ в паузе одной командой движка:
        // звук не успевает зазвучать при старте плеера.
        let resume = store.get_resume().await;
        let mut track = None;
        let mut paused = false;
        let mut selected = 0;
        if let Some((path, pos)) = resume {
            let path_buf = PathBuf::from(&path);
            if path_buf.exists() {
                engine.play_paused(&path, pos).await;
                paused = true;
                let path2 = path_buf.clone();
                let duration =
                    rap_engine::tokio::task::spawn_blocking(move || probe_duration(&path2))
                        .await
                        .ok()
                        .flatten();
                // Если файл есть в текущем списке — очередь продолжается с него,
                // иначе — играем одиночно (после него очередь заканчивается).
                let index = audio_files
                    .iter()
                    .position(|p| *p == path_buf)
                    .unwrap_or(audio_files.len());
                if let Some(i) = entries.iter().position(|e| e.path == path_buf) {
                    selected = i;
                }
                track = Some(Track {
                    path: path_buf,
                    duration,
                    index,
                    started: Instant::now(),
                });
            }
        }

        Self {
            strings,
            engine,
            store,
            stack: vec![root],
            entries,
            audio_files,
            selected,
            track,
            paused,
            volume,
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
        // Сохраняем позицию текущего трека при выходе
        match &self.track {
            Some(t) if !self.engine.is_empty() => {
                self.store
                    .set_resume(&t.path.display().to_string(), self.engine.get_current_pos())
                    .await;
            }
            _ => {
                self.store.clear_resume().await;
            }
        }
        self.engine.shutdown().await;
        self.store.shutdown();
    }

    async fn handle(&mut self, event: Event) {
        match event {
            // Press и Repeat (зажатая клавиша) — оба обрабатываем,
            // отбрасываем только Release
            Event::Key(k) if k.kind != KeyEventKind::Release => self.on_key(k.code).await,
            Event::Mouse(m) => self.on_mouse(m.kind, m.row, m.column).await,
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
            KeyCode::Backspace => self.go_back(),
            KeyCode::Left => self.seek_relative(-SEEK_STEP).await,
            KeyCode::Right => self.seek_relative(SEEK_STEP).await,
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char(' ') => self.toggle_pause().await,
            KeyCode::Char('+') | KeyCode::Char('=') => self.change_volume(VOLUME_STEP).await,
            KeyCode::Char('-') | KeyCode::Char('_') => self.change_volume(-VOLUME_STEP).await,
            _ => {}
        }
    }

    async fn change_volume(&mut self, delta: f32) {
        self.volume = (self.volume + delta).clamp(0.0, 1.0);
        self.engine.set_volume(self.volume).await;
        self.store.set_volume(self.volume).await;
    }

    async fn seek_relative(&mut self, offset_secs: i64) {
        if self.track.is_none() {
            return;
        }
        self.engine.seek_relative(offset_secs).await;
    }

    /// Перемотка по клику на полоске прогресса: позиция пропорциональна столбцу.
    async fn seek_to_click(&mut self, column: u16, width: u16) {
        let Some(duration) = self.track.as_ref().and_then(|t| t.duration) else {
            return;
        };
        if width == 0 {
            return;
        }
        let ratio = column as f32 / width as f32;
        let secs = (ratio * duration.as_secs_f32()) as u64;
        self.engine.seek_to(secs).await;
    }

    async fn on_mouse(&mut self, kind: MouseEventKind, row: u16, column: u16) {
        match kind {
            MouseEventKind::ScrollUp => self.selected = self.selected.saturating_sub(1),
            MouseEventKind::ScrollDown => {
                if self.selected + 1 < self.entries.len() {
                    self.selected += 1;
                }
            }
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                // Клик по полоске прогресса (последняя строка) — перемотка
                let (w, h) = crossterm::terminal::size().unwrap_or((80, 24));
                if row == h.saturating_sub(1) {
                    self.seek_to_click(column, w).await;
                    return;
                }
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

    fn go_back(&mut self) {
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
                // Очередь завершилась — в следующий раз начинаем с нуля
                self.track = None;
                self.paused = false;
                self.store.clear_resume().await;
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
            self.volume,
        )
    }
}
