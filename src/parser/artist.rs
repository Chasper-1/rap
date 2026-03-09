use crate::logger;
use crate::config::config::Config;

pub async fn process_and_log_metadata(
    raw_artist: Option<String>, 
    title: Option<String>, 
    album: Option<String>, 
    year: Option<String>, 
    genre: Option<String>, 
    comment: Option<String>
) -> (String, String) { // Убрали Vec<String> из возврата
    let config = Config::load().await;
    
    let artist_str = raw_artist.unwrap_or_else(|| "---".into());
    let title_str = title.unwrap_or_else(|| "---".into());
    let album_str = album.unwrap_or_else(|| "---".into());
    let year_str = year.unwrap_or_else(|| "---".into());
    let genre_str = genre.unwrap_or_else(|| "---".into());
    let comment_str = comment.unwrap_or_else(|| "---".into());

    let mut artists = Vec::new();
    let mut temp_str = artist_str.clone();
    
    for kw in &config.parser.feat_keywords {
        let pattern = format!(" {} ", kw);
        temp_str = temp_str.replace(&pattern, ", ").replace(kw, ", ");
    }

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

    let artists_fmt = artists.iter().enumerate()
        .map(|(i, name)| format!("[{}] {}", i + 1, name))
        .collect::<Vec<_>>()
        .join(", ");

    logger::log(&format!("Artists: {}", artists_fmt));
    logger::log(&format!("Track: {}", title_str));
    logger::log(&format!("Album: {}", album_str));
    logger::log(&format!("Year: {}", year_str));
    logger::log(&format!("Genre: {}", genre_str));
    logger::log(&format!("Comment: {}", comment_str));

    let final_artist = artists.first().cloned().unwrap_or(artist_str);
    (final_artist, title_str) // Возвращаем только два значения
}