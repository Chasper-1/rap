use crate::config::config::Config;
use crate::logger;
use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct FullMetadata {
    pub main_artists: Vec<String>,
    pub feat_artists: Vec<String>,
    pub title: String,
    pub album: String,
    pub year: String,
    pub genre: String,
    pub comment: String,
}

pub async fn process_and_log_metadata(
    raw_artist: Option<String>,
    title: Option<String>,
    album: Option<String>,
    year: Option<String>,
    genre: Option<String>,
    comment: Option<String>,
) -> (String, String) {
    let config = Config::global();
    let mut main_artists = Vec::new();
    let mut feat_artists = Vec::new();

    {
        let artist_raw = raw_artist.unwrap_or_else(|| "---".into());

        // 1. Ищем ГРАНИЦУ первого feat без создания artist_low
        let mut split_pos = artist_raw.len();
        let mut first_kw_len = 0;
        for kw in &config.parser.feat_keywords {
            if let Some(pos) = artist_raw.to_lowercase().find(&kw.to_lowercase()) {
                if pos < split_pos {
                    split_pos = pos;
                    first_kw_len = kw.len();
                }
            }
        }

        let (main_part_raw, feat_part_raw) = artist_raw.split_at(split_pos);
        let feat_part_clean = if feat_part_raw.len() > first_kw_len {
            &feat_part_raw[first_kw_len..]
        } else {
            ""
        };

        let process_zone = |input: &str, target: &mut Vec<String>| {
            let mut working_input = input.trim().trim_start_matches('.').trim().to_string();
            if working_input.is_empty() { return; }

            for exc in &config.parser.exceptions {
                if working_input.contains(exc) {
                    if !target.contains(exc) { target.push(exc.clone()); }
                    working_input = working_input.replace(exc, "|||");
                }
            }

            let mut frags = vec![working_input];
            for sep in &config.parser.separators {
                let mut next = Vec::new();
                for f in frags {
                    if f == "|||" { next.push(f); continue; }
                    next.extend(f.split(sep).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
                }
                frags = next;
            }

            for mut name in frags {
                if name == "|||" || name.is_empty() { continue; }
                
                for kw in &config.parser.feat_keywords {
                    // Сравниваем без создания новых строк в памяти
                    let is_match = name.eq_ignore_ascii_case(kw) || 
                                   (name.to_lowercase().ends_with('.') && name[..name.len()-1].eq_ignore_ascii_case(kw));
                    
                    if is_match {
                        name.clear();
                        break;
                    }

                    // Проверка префиксов без format!
                    let found_prefix = config.parser.feat_keywords.iter().any(|k| {
                        let n_low = name.to_lowercase();
                        let k_low = k.to_lowercase();
                        n_low.starts_with(&format!("{}.", k_low)) || n_low.starts_with(&format!("{} ", k_low))
                    });

                    if found_prefix {
                        if let Some(space_pos) = name.find(' ') {
                            name = name[space_pos..].trim().to_string();
                        } else if let Some(dot_pos) = name.find('.') {
                            name = name[dot_pos..].trim().to_string();
                        }
                        break;
                    }
                }

                let final_name = name.trim_matches('.').trim().to_string();
                if !final_name.is_empty() && !target.contains(&final_name) {
                    target.push(final_name);
                }
            }
        };

        process_zone(main_part_raw, &mut main_artists);
        process_zone(feat_part_clean, &mut feat_artists);
    }

    if main_artists.is_empty() { main_artists.push("---".into()); }

    let full_meta = FullMetadata {
        main_artists,
        feat_artists,
        title: title.unwrap_or_else(|| "---".into()),
        album: album.unwrap_or_else(|| "---".into()),
        year: year.map(|y| {
            let len = config.parser.year_length;
            if len > 0 && y.len() > len { y[..len].to_string() } else { y }
        }).unwrap_or_else(|| "---".into()),
        genre: genre.unwrap_or_else(|| "---".into()),
        comment: comment.unwrap_or_else(|| "---".into()),
    };

    if let Ok(json) = serde_json::to_string(&full_meta) {
        logger::log(&format!("METADATA_JSON:{}", json));
    }

    let res_title = full_meta.title;
    let res_main = full_meta.main_artists.into_iter().next().unwrap_or_else(|| "---".into());

    (res_main, res_title)
}