use serde::Deserialize;
use tokio::fs;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub parser: ParserConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ParserConfig {
    pub separators: Vec<String>,
    pub feat_keywords: Vec<String>,
}

impl Config {
    pub async fn load() -> Self {
        let content = fs::read_to_string("config.toml").await.unwrap_or_default();
        
        toml::from_str(&content).unwrap_or_else(|_| Config {
            parser: ParserConfig {
                separators: vec![",".into(), ";".into(), "/".into(), "&".into()],
                feat_keywords: vec!["feat.".into(), "ft.".into(), "feat".into()],
            },
        })
    }
}