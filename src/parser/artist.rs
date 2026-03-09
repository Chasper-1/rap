use crate::logger;
use crate::config::config;

pub fn process_and_log_artists(raw_artist: &str) -> Vec<String> {
    let config = config::Config::load();
    let mut artists = Vec::new();
    let mut temp_str = raw_artist.to_string();
    
    // Временно превращаем feat в разделитель для теста
    for kw in &config.parser.feat_keywords {
        let pattern = format!(" {} ", kw);
        temp_str = temp_str.replace(&pattern, ", ");
        temp_str = temp_str.replace(kw, ", ");
    }

    // Режем по разделителям
    let mut parts = vec![temp_str];
    for sep in &config.parser.separators {
        let mut next_parts = Vec::new();
        for p in parts {
            next_parts.extend(p.split(sep).map(|s| s.trim().to_string()));
        }
        parts = next_parts;
    }

    for name in parts {
        if !name.is_empty() && !artists.contains(&name) {
            artists.push(name);
        }
    }

    // Единственный лог на всю операцию
    if !artists.is_empty() {
        let formatted = artists.iter().enumerate()
            .map(|(i, name)| format!("[{}] {}", i + 1, name))
            .collect::<Vec<_>>()
            .join(", ");
        logger::log(&format!("Artists: {}", formatted));
    }

    artists
}