use serde::Deserialize;
use std::fs;
use std::sync::OnceLock;

static CONFIG: OnceLock<Config> = OnceLock::new();

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub parser: ParserConfig,
    pub ui: UiConfig, // ДОБАВИЛИ СЮДА
}

#[derive(Debug, Deserialize, Clone)]
pub struct ParserConfig {
    pub separators: Vec<String>,
    pub feat_keywords: Vec<String>,
    pub exceptions: Vec<String>,
    pub year_length: usize,
}

// НОВАЯ СТРУКТУРА ДЛЯ UI
#[derive(Debug, Deserialize, Clone)]
pub struct UiConfig {
    pub search_height: u16,
    pub cava_height: u16,
    pub step_offset: u16,
    pub line_x: u16,
    pub line_y: u16,
    pub line_width: u16,
}

impl Config {
    pub fn global() -> &'static Config {
        CONFIG.get_or_init(|| {
            let content = fs::read_to_string("config.toml").unwrap_or_default();
            toml::from_str(&content).unwrap_or(Config {
                parser: ParserConfig {
                    separators: vec![",".into(), ";".into(), "/".into(), "&".into()],
                    feat_keywords: vec!["feat.".into(), "ft.".into(), "feat".into()],
                    exceptions: vec!["AC/DC".into()],
                    year_length: 4,
                },
                // ДЕФОЛТНЫЕ НАСТРОЙКИ UI
                ui: UiConfig {
                    search_height: 3,
                    cava_height: 3,
                    step_offset: 1,
                    line_x: 0,
                    line_y: 0,
                    line_width: 0,
                },
            })
        })
    }
}