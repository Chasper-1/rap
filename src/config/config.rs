use serde::Deserialize;
use std::fs;
use std::sync::OnceLock;

static CONFIG: OnceLock<Config> = OnceLock::new();
static CONFIG_ERROR: OnceLock<Option<String>> = OnceLock::new();

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub parser: ParserConfig,
    pub ui: UiConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingConfig {
    pub max_logs: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ParserConfig {
    pub separators: Vec<String>,
    pub feat_keywords: Vec<String>,
    pub exceptions: Vec<String>,
    pub year_length: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UiConfig {
    pub search_height: u16,
    pub cava_height: u16,
    pub step_offset: u16,
    pub line_x: u16,
    pub line_y: u16,
    pub line_width: u16,
    pub search_box_x: u16,
    pub search_box_y: u16,
    pub search_box_width: u16,
    pub search_box_height: u16,
    pub library_x: u16,
    pub library_y: u16,
    pub library_width: u16,
    pub library_height: u16,
    pub logo_x: u16,
    pub logo_y: u16,
}

impl Config {
    pub fn global() -> &'static Config {
        CONFIG.get_or_init(|| {
            let content = fs::read_to_string("config.toml").unwrap_or_else(|e| {
                let err = format!("CRITICAL: Ошибка чтения config.toml: {}", e);
                let _ = fs::write("rmpt.log", &err);
                String::new()
            });

            // Чистим запятые в концах строк, чтобы toml-rs не доёбывался
            let cleaned_content = content
                .lines()
                .map(|line| {
                    let trimmed = line.trim_end();
                    if trimmed.ends_with(',') {
                        &trimmed[..trimmed.len() - 1]
                    } else {
                        trimmed
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            match toml::from_str::<Config>(&cleaned_content) {
                Ok(cfg) => cfg,
                Err(e) => {
                    let err_msg =
                        format!("TOML ERROR: {}\nПроверь, все ли поля заполнены в [ui]!", e);
                    let _ = fs::write("rmpt.log", &err_msg);

                    // Сохраняем ошибку во второй OnceLock
                    let _ = CONFIG_ERROR.set(Some(err_msg));

                    // Возвращаем дефолт, чтобы прога не упала сразу
                    Self::default_vals()
                }
            }
        })
    }

    pub fn get_last_error() -> Option<String> {
        CONFIG_ERROR.get().cloned().flatten()
    }

    fn default_vals() -> Self {
        Config {
            parser: ParserConfig {
                separators: vec![",".into(), ";".into(), "/".into(), "&".into()],
                feat_keywords: vec!["feat.".into(), "ft.".into(), "feat".into()],
                exceptions: vec!["AC/DC".into()],
                year_length: 4,
            },
            ui: UiConfig {
                search_height: 3,
                cava_height: 3,
                step_offset: 1,
                line_x: 2,
                line_y: 2,
                line_width: 20,
                search_box_x: 2,
                search_box_y: 1,
                search_box_width: 40,
                search_box_height: 3,
                library_x: 0,
                library_y: 5,
                library_width: 30,
                library_height: 15,
                logo_x: 1,
                logo_y: 35,
            },
            logging: LoggingConfig { max_logs: 5 },
        }
    }
}
