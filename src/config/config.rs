use serde::Deserialize;
use std::fs;
use std::sync::OnceLock;

static CONFIG: OnceLock<Config> = OnceLock::new();

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub parser: ParserConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ParserConfig {
    pub separators: Vec<String>,
    pub feat_keywords: Vec<String>,
    pub exceptions: Vec<String>,
    pub year_length: usize,
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
            })
        })
    }
}