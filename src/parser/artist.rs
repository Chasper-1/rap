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
        let artist_low = artist_raw.to_lowercase();

        let mut split_pos = artist_raw.len();
        let mut first_kw_len = 0;
        for kw in &config.parser.feat_keywords {
            let kw_l = kw.to_lowercase();
            if let Some(pos) = artist_low.find(&kw_l) {
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
            let trimmed = input.trim().trim_start_matches('.').trim();
            if trimmed.is_empty() {
                return;
            }

            let mut working_input = trimmed.to_string();
            for exc in &config.parser.exceptions {
                if working_input.contains(exc) {
                    if !target.contains(exc) {
                        target.push(exc.clone());
                    }
                    working_input = working_input.replace(exc, "|||");
                }
            }

            let mut frags = vec![working_input];
            for sep in &config.parser.separators {
                let mut next = Vec::new();
                for f in frags {
                    if f == "|||" {
                        next.push(f);
                        continue;
                    }
                    next.extend(
                        f.split(sep)
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty()),
                    );
                }
                frags = next;
            }

            for mut name in frags {
                if name == "|||" || name.is_empty() {
                    continue;
                }
                for kw in &config.parser.feat_keywords {
                    let kw_low = kw.to_lowercase();
                    let name_low = name.to_lowercase();
                    if name_low == kw_low || name_low == format!("{}.", kw_low) {
                        name.clear();
                        break;
                    }
                    for p in &[format!("{}.", kw_low), format!("{} ", kw_low), kw_low] {
                        if name_low.starts_with(p) {
                            name = name[p.len()..].trim().to_string();
                            break;
                        }
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

    if main_artists.is_empty() {
        main_artists.push("---".into());
    }

    let full_meta = FullMetadata {
        main_artists,
        feat_artists,
        title: title.unwrap_or_else(|| "---".into()),
        album: album.unwrap_or_else(|| "---".into()),
        year: year
            .map(|y| {
                let len = config.parser.year_length;
                if len > 0 && y.len() > len {
                    y[..len].to_string()
                } else {
                    y
                }
            })
            .unwrap_or_else(|| "---".into()),
        genre: genre.unwrap_or_else(|| "---".into()),
        comment: comment.unwrap_or_else(|| "---".into()),
    };

    // 2. Выводим JSON. После этой строки данные должны сдохнуть.
    if let Ok(json) = serde_json::to_string(&full_meta) {
        logger::log(&format!("METADATA_JSON:{}", json));
    }

    // 3. УНИЧТОЖАЕМ ВСЁ ЛИШНЕЕ.
    // Забираем только то, что возвращаем. Остальное (album, year, genre, comment) Rust дропает ТУТ ЖЕ.
    let res_title = full_meta.title;
    let res_main = full_meta
        .main_artists
        .into_iter()
        .next()
        .unwrap_or_else(|| "---".into());

    (res_main, res_title)
}
