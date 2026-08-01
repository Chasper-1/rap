//! Асинхронное хранилище настроек на SQLite.
//!
//! Соединение с БД живёт в отдельном потоке-воркере: запросы приходят
//! по каналу (tokio mpsc), ответы — через oneshot. Мутексы не нужны:
//! sqlite трогается только из своего потока, сериализация — самим каналом.
//! Интерфейс наружу — обычный async API.
//!
//! Файл БД: `$XDG_DATA_HOME/rap/rap.db` (по умолчанию `~/.local/share/rap/rap.db`).

pub mod core;

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use tokio::sync::{mpsc, oneshot};

// Ключи настроек в БД.
const KEY_VOLUME: &str = "volume";
const KEY_LANG: &str = "lang";
const KEY_RESUME_PATH: &str = "resume_path";
const KEY_RESUME_POS: &str = "resume_pos";

enum Cmd {
    GetVolume(oneshot::Sender<Option<f32>>),
    SetVolume(f32),
    GetLang(oneshot::Sender<Option<String>>),
    SetLang(String),
    GetResume(oneshot::Sender<Option<(String, u64)>>),
    SetResume(String, u64),
    ClearResume,
}

// Асинхронный доступ к настройкам.
pub struct Store {
    tx: Option<mpsc::UnboundedSender<Cmd>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Store {
    // Открывает БД в стандартном месте (XDG) и запускает воркер.
    // При любой ошибке возвращает «пустое» хранилище: все методы
    // безопасно ничего не делают.
    pub fn open() -> Self {
        Self::open_at(&default_db_path())
    }

    // Открывает БД по конкретному пути (используется в тестах).
    pub fn open_at(path: &Path) -> Self {
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        if std::fs::create_dir_all(dir).is_err() {
            return Self {
                tx: None,
                worker: None,
            };
        }
        let conn = match core::open(path) {
            Ok(c) => c,
            Err(_) => {
                return Self {
                    tx: None,
                    worker: None,
                };
            }
        };

        let (tx, rx) = mpsc::unbounded_channel::<Cmd>();
        let worker = std::thread::spawn(move || {
            if let Err(e) = worker(rx, conn) {
                panic!("воркер хранилища завершился с ошибкой: {e}");
            }
        });
        Self {
            tx: Some(tx),
            worker: Some(worker),
        }
    }

    // Закрывает канал и дожидается завершения воркера.
    pub fn shutdown(&mut self) {
        self.tx = None;
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                eprintln!("[store] воркер завершился с ошибкой");
            }
        }
    }

    pub async fn get_volume(&self) -> Option<f32> {
        let (tx, rx) = oneshot::channel();
        if self.send(Cmd::GetVolume(tx)) {
            rx.await.ok().flatten()
        } else {
            None
        }
    }

    pub async fn set_volume(&self, volume: f32) {
        self.send(Cmd::SetVolume(volume.clamp(0.0, 1.0)));
    }

    pub async fn get_lang(&self) -> Option<String> {
        let (tx, rx) = oneshot::channel();
        if self.send(Cmd::GetLang(tx)) {
            rx.await.ok().flatten()
        } else {
            None
        }
    }

    pub async fn set_lang(&self, lang: &str) {
        self.send(Cmd::SetLang(lang.to_string()));
    }

    // Сохранённая позиция воспроизведения: (путь к файлу, секунда).
    pub async fn get_resume(&self) -> Option<(String, u64)> {
        let (tx, rx) = oneshot::channel();
        if self.send(Cmd::GetResume(tx)) {
            rx.await.ok().flatten()
        } else {
            None
        }
    }

    pub async fn set_resume(&self, path: &str, pos_secs: u64) {
        self.send(Cmd::SetResume(path.to_string(), pos_secs));
    }

    pub async fn clear_resume(&self) {
        self.send(Cmd::ClearResume);
    }

    fn send(&self, cmd: Cmd) -> bool {
        match &self.tx {
            Some(tx) => tx.send(cmd).is_ok(),
            None => false,
        }
    }
}

// Поток-воркер: единственный владелец соединения.
//
// Ошибки БД прокидываются наверх (`?`) — поток завершается и паникует
// в точке запуска с понятным текстом. Ошибка отправки ответа
// (`tx.send`) означает, что клиент мёртв: поток немедленно выходит.
fn worker(mut rx: mpsc::UnboundedReceiver<Cmd>, conn: Connection) -> rusqlite::Result<()> {
    while let Some(cmd) = rx.blocking_recv() {
        match cmd {
            Cmd::GetVolume(tx) => {
                let value = core::get(&conn, KEY_VOLUME)?
                    .and_then(|s| s.parse::<f32>().ok())
                    .map(|v| v.clamp(0.0, 1.0));
                if tx.send(value).is_err() {
                    break;
                }
            }
            Cmd::SetVolume(v) => {
                core::set(&conn, KEY_VOLUME, &v.to_string())?;
            }
            Cmd::GetLang(tx) => {
                let value = core::get(&conn, KEY_LANG)?;
                if tx.send(value).is_err() {
                    break;
                }
            }
            Cmd::SetLang(lang) => {
                core::set(&conn, KEY_LANG, &lang)?;
            }
            Cmd::GetResume(tx) => {
                let value = match core::get(&conn, KEY_RESUME_PATH)? {
                    Some(path) => core::get(&conn, KEY_RESUME_POS)?
                        .and_then(|s| s.parse::<u64>().ok())
                        .map(|pos| (path, pos)),
                    None => None,
                };
                if tx.send(value).is_err() {
                    break;
                }
            }
            Cmd::SetResume(path, pos) => {
                core::set(&conn, KEY_RESUME_PATH, &path)?;
                core::set(&conn, KEY_RESUME_POS, &pos.to_string())?;
            }
            Cmd::ClearResume => {
                core::set(&conn, KEY_RESUME_PATH, "")?;
                core::set(&conn, KEY_RESUME_POS, "")?;
            }
        }
    }
    Ok(())
}

// Стандартное место файла БД: `$XDG_DATA_HOME/rap/rap.db`.
pub fn default_db_path() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(|h| PathBuf::from(h).join(".local/share"))
                .unwrap_or_else(|| std::env::temp_dir())
        });
    base.join("rap").join("rap.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rap_store_{name}_{}", std::process::id()))
    }

    #[tokio::test]
    async fn volume_roundtrip() {
        let path = test_path("vol");
        let mut store = Store::open_at(&path);
        assert_eq!(store.get_volume().await, None);
        store.set_volume(0.7).await;
        assert_eq!(store.get_volume().await, Some(0.7));
        store.shutdown();
        std::fs::remove_file(&path).expect("тестовый файл не удалился");
    }

    #[tokio::test]
    async fn volume_clamped() {
        let path = test_path("clamp");
        let mut store = Store::open_at(&path);
        store.set_volume(5.0).await;
        assert_eq!(store.get_volume().await, Some(1.0));
        store.shutdown();
        std::fs::remove_file(&path).expect("тестовый файл не удалился");
    }

    #[tokio::test]
    async fn lang_roundtrip() {
        let path = test_path("lang");
        let mut store = Store::open_at(&path);
        assert_eq!(store.get_lang().await, None);
        store.set_lang("en").await;
        assert_eq!(store.get_lang().await, Some("en".to_string()));
        store.shutdown();
        std::fs::remove_file(&path).expect("тестовый файл не удалился");
    }

    #[tokio::test]
    async fn store_survives_reopen() {
        let path = test_path("reopen");
        {
            let mut store = Store::open_at(&path);
            store.set_volume(0.4).await;
            store.set_lang("ru").await;
            store.shutdown();
        }
        let mut store = Store::open_at(&path);
        assert_eq!(store.get_volume().await, Some(0.4));
        assert_eq!(store.get_lang().await, Some("ru".to_string()));
        store.shutdown();
        std::fs::remove_file(&path).expect("тестовый файл не удалился");
    }

    #[tokio::test]
    async fn resume_roundtrip() {
        let path = test_path("resume");
        let mut store = Store::open_at(&path);
        assert_eq!(store.get_resume().await, None);
        store.set_resume("/music/song.mp3", 42).await;
        assert_eq!(
            store.get_resume().await,
            Some(("/music/song.mp3".to_string(), 42))
        );
        store.clear_resume().await;
        assert_eq!(store.get_resume().await, None);
        store.shutdown();
        std::fs::remove_file(&path).expect("тестовый файл не удалился");
    }

    #[tokio::test]
    async fn resume_survives_reopen() {
        let path = test_path("resume_reopen");
        {
            let mut store = Store::open_at(&path);
            store.set_resume("/music/song.flac", 7).await;
            store.shutdown();
        }
        let mut store = Store::open_at(&path);
        assert_eq!(
            store.get_resume().await,
            Some(("/music/song.flac".to_string(), 7))
        );
        store.shutdown();
        std::fs::remove_file(&path).expect("тестовый файл не удалился");
    }
}
