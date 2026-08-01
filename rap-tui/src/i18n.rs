use std::path::{Path, PathBuf};

/// Все тексты интерфейса.
///
/// Загружаются из файла `lang_<код>.txt` (рядом с исполняемым файлом
/// или в текущей папке), либо используются встроенные дефолты.
/// Формат файла: `ключ=значение`, строки с `#` — комментарии.
/// Чтобы сделать перевод: скопируй файл, переименуй `lang_<код>.txt`
/// и замени текст.
#[derive(Debug, Clone)]
pub struct Strings {
    pub error_bad_path: String,
    pub error_is_file: String,
    pub header: String,
    pub empty_dir: String,
    pub now_playing: String,
    pub paused: String,
    pub volume: String,
}

impl Default for Strings {
    fn default() -> Self {
        Self::defaults()
    }
}

impl Strings {
    /// Встроенный русский перевод.
    pub fn defaults() -> Self {
        Self {
            error_bad_path: "Папка не найдена: {path}".into(),
            error_is_file: "Указанный путь — это файл, нужна папка".into(),
            header: "Папка: {path}".into(),
            empty_dir: "Пусто".into(),
            now_playing: "Сейчас играет: ".into(),
            paused: " (пауза)".into(),
            volume: "Громкость: {vol}%".into(),
        }
    }

    /// Загружает перевод для кода языка (например, "ru").
    /// Если файл не найден — встроенные дефолты.
    pub fn load(lang: &str) -> Self {
        let mut s = Self::defaults();
        if let Some(path) = find_lang_file(lang) {
            s = Self::load_from_file(&path);
        }
        s
    }

    /// Загружает перевод из конкретного файла; отсутствующие ключи
    /// остаются дефолтными.
    pub fn load_from_file(path: &Path) -> Self {
        let mut s = Self::defaults();
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return s,
        };
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            s.apply(key.trim(), value.trim());
        }
        s
    }

    fn apply(&mut self, key: &str, value: &str) {
        match key {
            "error_bad_path" => self.error_bad_path = value.into(),
            "error_is_file" => self.error_is_file = value.into(),
            "header" => self.header = value.into(),
            "empty_dir" => self.empty_dir = value.into(),
            "now_playing" => self.now_playing = value.into(),
            "paused" => self.paused = value.into(),
            "volume" => self.volume = value.into(),
            _ => {}
        }
    }

    pub fn error_bad_path_with(&self, path: &str) -> String {
        self.error_bad_path.replace("{path}", path)
    }

    pub fn header_with(&self, path: &str) -> String {
        self.header.replace("{path}", path)
    }

    pub fn volume_with(&self, volume_pct: u32) -> String {
        self.volume.replace("{vol}", &volume_pct.to_string())
    }
}

/// Ищет `lang_<код>.txt`: рядом с исполняемым файлом, затем в текущей папке.
fn find_lang_file(lang: &str) -> Option<PathBuf> {
    let file = format!("lang_{lang}.txt");
    if let Ok(exe) = std::env::current_exe() {
        let p = exe.parent()?.join(&file);
        if p.exists() {
            return Some(p);
        }
    }
    let p = std::env::current_dir().ok()?.join(&file);
    if p.exists() { Some(p) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_not_empty() {
        let s = Strings::defaults();
        assert!(!s.header.is_empty());
        assert!(s.header.contains("{path}"));
        assert!(!s.now_playing.is_empty());
    }

    #[test]
    fn load_from_file_applies_keys() {
        let dir = std::env::temp_dir().join(format!("rap_i18n_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("тестовая папка не создалась");
        let file = dir.join("lang_test.txt");
        std::fs::write(
            &file,
            "# comment\nnow_playing=Playing:\n\nunknown_key=ignored\nheader=Dir: {path}\n",
        )
        .unwrap();

        let s = Strings::load_from_file(&file);
        assert_eq!(s.now_playing, "Playing:");
        assert_eq!(s.header, "Dir: {path}");
        // не тронутые ключи — дефолты
        assert_eq!(s.paused, Strings::defaults().paused);
        std::fs::remove_dir_all(&dir).expect("тестовая папка не удалилась");
    }

    #[test]
    fn load_missing_lang_uses_defaults() {
        let s = Strings::load("zz-no-such-lang");
        assert_eq!(s.header, Strings::defaults().header);
    }

    #[test]
    fn format_with_path() {
        let s = Strings::defaults();
        assert_eq!(
            s.header_with("/tmp/music"),
            s.header.replace("{path}", "/tmp/music")
        );
        assert!(s.error_bad_path_with("/x").contains("/x"));
    }
}
