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
    pub input: InputConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct InputConfig {
    pub quit: Vec<String>,
    pub toggle_pause: Vec<String>,
    pub stop: Vec<String>,
    pub seek_start: Vec<String>,
    pub vol_up: Vec<String>,
    pub vol_down: Vec<String>,
    pub forward: Vec<String>,
    pub backward: Vec<String>,

    #[serde(rename = "forward-step")]
    pub forward_step: i64,

    #[serde(rename = "backward-step")]
    pub backward_step: i64,
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
    pub cava_show: bool, // <-- Управление видимостью CAVA без удаления высоты
    pub search_height: u16,
    pub cava_height: u16,
    pub cava_fall_speed: f32,
    pub cava_exponent: f32,
    pub cava_sensitivity: f32,
    pub cava_noise_gate: f32,
    pub cava_attack: f32,
    pub eq_low: f32,
    pub eq_mid: f32,
    pub eq_high: f32,
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
    pub colors: UiColors,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UiColors {
    pub logo: [u8; 3],
    pub search_label: [u8; 3],
    pub regex_label: [u8; 3],
    pub library_label: [u8; 3],
    pub buttons: [u8; 3],
}

impl Config {
    pub fn global() -> &'static Config {
        CONFIG.get_or_init(|| {
            let content = fs::read_to_string("config.jsonc").unwrap_or_else(|_| {
                crate::logger::log("CRITICAL: config.jsonc не найден");
                String::new()
            });

            match json5::from_str::<Config>(&content) {
                Ok(cfg) => cfg,
                Err(e) => {
                    let err_msg = format!("CONFIG ERROR (JSON5): {}", e);
                    crate::logger::log(&err_msg);

                    let _ = CONFIG_ERROR.set(Some(err_msg));
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
                cava_show: true, // <-- Дефолтное значение флага
                cava_height: 3,
                cava_fall_speed: 0.88,
                cava_exponent: 0.8,
                cava_sensitivity: 1.0,
                cava_noise_gate: 0.01,
                cava_attack: 0.3,
                eq_low: 1.0,
                eq_mid: 1.0,
                eq_high: 1.0,
                search_height: 4,
                step_offset: 3,
                line_x: 1,
                line_y: 28,
                line_width: 2,
                search_box_x: 1,
                search_box_y: 0,
                search_box_width: 45,
                search_box_height: 5,
                library_x: 4,
                library_y: 4,
                library_width: 42,
                library_height: 36,
                logo_x: 0,
                logo_y: 29,
                colors: UiColors {
                    logo: [167, 192, 128],
                    search_label: [200, 200, 200],
                    regex_label: [200, 200, 200],
                    library_label: [200, 200, 200],
                    buttons: [167, 192, 128],
                },
            },
            input: InputConfig {
                quit: vec!["q".into(), "Esc".into()],
                toggle_pause: vec!["Space".into()],
                stop: vec!["Delete".into()],
                seek_start: vec!["Home".into()],
                vol_up: vec!["=".into(), "+".into()],
                vol_down: vec!["-".into(), "_".into()],
                forward: vec!["Right".into(), "f".into()],
                backward: vec!["Left".into(), "b".into()],
                forward_step: 5,
                backward_step: 5,
            },
            logging: LoggingConfig { max_logs: 5 },
        }
    }
}
