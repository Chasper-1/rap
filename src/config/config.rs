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
    pub search_box_x: u16,
    pub search_box_y: u16,
    pub search_box_width: u16,
    pub search_box_height: u16,
}

impl Config {
    pub fn global() -> &'static Config {
        CONFIG.get_or_init(|| {
            let content = fs::read_to_string("config.toml").unwrap_or_else(|e| {
                let _ = fs::write("rmpt.log", format!("CRITICAL: Ошибка чтения файла config.toml: {}", e));
                String::new()
            });

            match toml::from_str::<Config>(&content) {
                Ok(cfg) => cfg,
                Err(e) => {
                    // ВОТ ЭТО ТО САМОЕ ОЧЕВИДНОЕ ДЕЙСТВИЕ
                    let error_msg = format!("TOML ERROR: Конфиг - говно. Ошибка: {}\nПроверь, все ли поля из UiConfig есть в [ui]!", e);
                    let _ = fs::write("rmpt.log", &error_msg);

                    // Возвращаем дефолт, чтобы не упасть, но в лог уже насрали
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
                            line_x: 2,         // Ставим не нулевые, чтобы хоть что-то
                            line_y: 2,         // было видно, если лог проигноришь
                            line_width: 20,
                            search_box_x: 2,
                            search_box_y: 1,
                            search_box_width: 40,
                            search_box_height: 3,
                        },
                    }
                }
            }
        })
    }
}
