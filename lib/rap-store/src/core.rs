//! Синхронный слой над SQLite: схема и базовые операции.
//! Вызывается только из потока-воркера `Store` (см. lib.rs), поэтому
//! мутексы не нужны — соединение всегда принадлежит одному потоку.

use rusqlite::Connection;
use std::path::Path;

// Создаёт схему БД, если её ещё нет.
pub fn init(conn: &mut Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS settings (
             key   TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );",
    )
}

// Читает значение настройки по ключу.
pub fn get(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = stmt.query([key])?;
    match rows.next()? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

// Записывает значение настройки (upsert по ключу).
pub fn set(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

// Открывает соединение с БД и инициализирует схему.
pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let mut conn = Connection::open(path)?;
    init(&mut conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn(name: &str) -> Connection {
        let dir = std::env::temp_dir().join(format!("rap_store_core_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("тестовая папка не создалась");
        let path = dir.join(format!("{name}.db"));
        if path.exists() {
            std::fs::remove_file(&path).expect("тестовый файл не удалился");
        }
        open(&path).expect("открытие БД")
    }

    #[test]
    fn init_idempotent() {
        let mut conn = test_conn("init_idempotent");
        init(&mut conn).expect("повторный init не должен падать");
    }

    #[test]
    fn set_get_roundtrip() {
        let conn = test_conn("set_get_roundtrip");
        set(&conn, "volume", "0.7").unwrap();
        assert_eq!(get(&conn, "volume").unwrap(), Some("0.7".to_string()));
    }

    #[test]
    fn get_missing_is_none() {
        let conn = test_conn("get_missing_is_none");
        assert_eq!(get(&conn, "nope").unwrap(), None);
    }

    #[test]
    fn set_overwrites() {
        let conn = test_conn("set_overwrites");
        set(&conn, "lang", "ru").unwrap();
        set(&conn, "lang", "en").unwrap();
        assert_eq!(get(&conn, "lang").unwrap(), Some("en".to_string()));
    }
}
