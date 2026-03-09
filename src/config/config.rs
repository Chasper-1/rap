use serde::Deserialize;
use std::fs;

// Добавляем #[derive(Deserialize)], теперь это будет работать
#[derive(Debug, Deserialize)]
pub struct Config {
    pub parser: ParserConfig,
}

#[derive(Debug, Deserialize)]
pub struct ParserConfig {
    pub separators: Vec<String>,
    pub feat_keywords: Vec<String>,
}

impl Config {
    pub fn load() -> Self {
        // Читаем из корня проекта, где лежит config.toml
        let content = fs::read_to_string("config.toml").unwrap_or_default();
        
        toml::from_str(&content).unwrap_or_else(|_| {
            // Дефолты на случай, если файла нет или он кривой
            Config {
                parser: ParserConfig {
                    separators: vec![",".into(), ";".into(), "/".into(), "&".into()],
                    feat_keywords: vec!["feat.".into(), "ft.".into()],
                }
            }
        })
    }
}