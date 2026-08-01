use std::fs;
use std::path::{Path, PathBuf};

// Запись списка папки.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

// Расширения аудиофайлов, которые показываем и играем.
pub const AUDIO_EXTS: [&str; 7] = ["mp3", "flac", "wav", "ogg", "opus", "m4a", "aac"];

// Есть ли у имени файла аудио-расширение (регистронезависимо).
pub fn is_audio(name: &str) -> bool {
    let ext = match name.rsplit_once('.') {
        Some((_, e)) => e,
        None => return false,
    };
    AUDIO_EXTS.iter().any(|a| a.eq_ignore_ascii_case(ext))
}

// Сканирует папку: папки первыми, затем аудиофайлы; внутри групп — по имени.
// Неаудио файлы пропускаются. Без индексации и кэшей.
pub fn scan_dir(dir: &Path) -> std::io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for item in fs::read_dir(dir)? {
        let item = match item {
            Ok(i) => i,
            Err(_) => continue,
        };
        let file_type = match item.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        let is_dir = file_type.is_dir();
        let name = item.file_name().to_string_lossy().into_owned();
        if !is_dir && !is_audio(&name) {
            continue;
        }
        entries.push(Entry {
            name,
            path: item.path(),
            is_dir,
        });
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

// Список аудиофайлов из сканированной папки (без папок), по порядку списка.
pub fn audio_list(entries: &[Entry]) -> Vec<PathBuf> {
    entries
        .iter()
        .filter(|e| !e.is_dir)
        .map(|e| e.path.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_audio_extensions() {
        assert!(is_audio("song.mp3"));
        assert!(is_audio("Song.FLAC"));
        assert!(is_audio("a.opus"));
        assert!(!is_audio("readme.txt"));
        assert!(!is_audio("noext"));
        assert!(!is_audio("song.mp3.exe"));
    }

    #[test]
    fn scan_dir_order_and_filter() {
        let dir = std::env::temp_dir().join(format!("rap_scan_{}", std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir).expect("тестовая папка не удалилась");
        }
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir(dir.join("zz_folder")).unwrap();
        fs::create_dir(dir.join("aa_folder")).unwrap();
        fs::write(dir.join("b.mp3"), b"x").unwrap();
        fs::write(dir.join("a.wav"), b"x").unwrap();
        fs::write(dir.join("ignore.txt"), b"x").unwrap();

        let entries = scan_dir(&dir).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["aa_folder", "zz_folder", "a.wav", "b.mp3"]);

        let audio = audio_list(&entries);
        assert_eq!(audio.len(), 2);
        assert!(audio[0].ends_with("a.wav"));
        fs::remove_dir_all(&dir).expect("тестовая папка не удалилась");
    }

    #[test]
    fn scan_empty_dir() {
        let dir = std::env::temp_dir().join(format!("rap_scan_empty_{}", std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir).expect("тестовая папка не удалилась");
        }
        fs::create_dir_all(&dir).unwrap();
        assert!(scan_dir(&dir).unwrap().is_empty());
        fs::remove_dir_all(&dir).expect("тестовая папка не удалилась");
    }
}
