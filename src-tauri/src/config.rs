use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageMode { Auto, Ru, Uk, En }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelSize {
    Tiny, Base, Small, Medium,
    #[serde(rename = "large-v3")]
    LargeV3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsertionMode { Type, ClipboardPaste }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub hotkey: String,
    pub language_mode: String,
    pub model_size: String,
    pub input_device: String,
    pub show_transcription_overlay: bool,
    pub live_preview: bool,
    pub fast_recognition: bool,
    pub cloud_enabled: bool,
    pub cloud_api_key: String,
    pub auto_punctuation: bool,
    pub grammar_correction: bool,
    pub voice_commands: bool,
    pub custom_dictionary: Vec<String>,
    pub insertion_mode: String,
    pub start_with_windows: bool,
    pub minimize_to_tray: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: "RightCtrl".to_string(),
            language_mode: "auto".to_string(),
            model_size: "tiny".to_string(),
            input_device: String::new(),
            show_transcription_overlay: true,
            live_preview: false,
            fast_recognition: false,
            cloud_enabled: false,
            cloud_api_key: String::new(),
            auto_punctuation: true,
            grammar_correction: true,
            voice_commands: true,
            custom_dictionary: vec![],
            insertion_mode: "clipboard_paste".to_string(),
            start_with_windows: false,
            minimize_to_tray: true,
        }
    }
}

pub struct ConfigState(pub Mutex<AppConfig>);

fn config_path() -> PathBuf {
    let dir = dirs_next::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("DictaFlow");
    let _ = fs::create_dir_all(&dir);
    dir.join("config.json")
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(cfg) = serde_json::from_str::<AppConfig>(&raw) {
            return cfg;
        }
    }
    AppConfig::default()
}

pub fn save_config(cfg: &AppConfig) -> anyhow::Result<()> {
    let raw = serde_json::to_string_pretty(cfg)?;
    let path = config_path();
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, raw)?;
    fs::rename(temporary, path)?;
    Ok(())
}

pub fn validate(cfg: &AppConfig) -> Result<(), String> {
    if crate::hotkey::key_from_name(&cfg.hotkey).is_none() { return Err("Неподдерживаемая горячая клавиша".into()); }
    if !matches!(cfg.model_size.as_str(), "tiny" | "base" | "small" | "medium" | "large-v3") { return Err("Неизвестная модель".into()); }
    if !matches!(cfg.language_mode.as_str(), "auto" | "ru" | "uk" | "en") { return Err("Неизвестный язык".into()); }
    if !matches!(cfg.insertion_mode.as_str(), "type" | "clipboard_paste") { return Err("Неизвестный способ вставки".into()); }
    Ok(())
}
