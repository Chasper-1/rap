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

        // 1. Ищем ГРАНИЦУ первого feat для разделения зон
        let mut split_pos = artist_raw.len();
        let mut first_kw_len = 0;
        for kw in &config.parser.feat_keywords {
            if let Some(pos) = artist_low.find(&kw.to_lowercase()) {
                if pos < split_pos {
                    split_pos = pos;
                    first_kw_len = kw.len();
                }
            }
        }

        let (main_part_raw, feat_part_raw) = artist_raw.split_at(split_pos);
        // Убираем хвост первого триггера, если он есть
        let feat_part_clean = if feat_part_raw.len() > first_kw_len {
            &feat_part_raw[first_kw_len..]
        } else {
            ""
        };

        // 2. Обработка зон с жесткой фильтрацией мусора
        let process_zone = |input: &str, target: &mut Vec<String>| {
            let trimmed_input = input.trim().trim_start_matches('.').trim();
            if trimmed_input.is_empty() {
                return;
            }

            let mut working_input = trimmed_input.to_string();

            // Вынимаем исключения
            for exc in &config.parser.exceptions {
                if working_input.contains(exc) {
                    if !target.contains(exc) {
                        target.push(exc.clone());
                    }
                    working_input = working_input.replace(exc, "|||");
                }
            }

            // Дробим по сепараторам
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

            // 3. ФИНАЛЬНЫЙ ФИЛЬТР: чистим каждый фрагмент от feat-слов
            for f in frags {
                if f == "|||" || f.is_empty() {
                    continue;
                }

                let mut name = f.clone();
                let mut is_junk = false;

                for kw in &config.parser.feat_keywords {
                    let kw_low = kw.to_lowercase();
                    let name_low = name.to_lowercase();

                    // Если фрагмент целиком является feat-словом
                    if name_low == kw_low || name_low == format!("{}.", kw_low) {
                        is_junk = true;
                        break;
                    }

                    // Если feat затесался внутри фрагмента (feat. Казян)
                    for pattern in &[
                        format!("{}.", kw_low),
                        format!("{} ", kw_low),
                        kw_low.clone(),
                    ] {
                        if name_low.starts_with(pattern) {
                            name = name[pattern.len()..].trim().to_string();
                            break;
                        }
                    }
                }

                if is_junk || name.is_empty() {
                    continue;
                }

                // Чистим артефакты (точки по краям)
                let final_name = name.trim_matches('.').trim().to_string();

                if final_name.contains(' ') && !config.parser.exceptions.contains(&final_name) {
                    logger::log(&format!(
                        "WARNING: Possible missing separator in '{}'",
                        final_name
                    ));
                }

                if !target.contains(&final_name) && !final_name.is_empty() {
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

    let final_year = year
        .map(|y| {
            let len = config.parser.year_length;
            if len > 0 && y.len() > len {
                y[..len].to_string()
            } else {
                y
            }
        })
        .unwrap_or_else(|| "---".into());

    // Создаем финальный объект. Владение (move) переходит сюда.
    let full_meta = FullMetadata {
        main_artists, // Передали и забыли
        feat_artists, // Передали и забыли
        title: title.unwrap_or_else(|| "---".into()),
        album: album.unwrap_or_else(|| "---".into()),
        year: final_year,
        genre: genre.unwrap_or_else(|| "---".into()),
        comment: comment.unwrap_or_else(|| "---".into()),
    };

    // Вывели JSON в лог — это единственное, что остается во внешнем мире
    if let Ok(json) = serde_json::to_string(&full_meta) {
        logger::log(&format!("METADATA_JSON:{}", json));
    }

    // Достаем только то, что нужно для возврата, остальное FullMetadata уничтожается здесь
    let res_main = full_meta.main_artists.first().cloned().unwrap_or_else(|| "---".into());
        (res_main, full_meta.title)
    // Все промежуточные данные (config, tokens, и т.д.) здесь уже мертвы.
}
