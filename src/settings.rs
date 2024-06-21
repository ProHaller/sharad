use crate::error::SharadError;
use serde::{Deserialize, Serialize};
use std::fs;

const SETTINGS_FILE: &str = "./data/logs/settings.json";

#[derive(Serialize, Deserialize)]
pub struct Settings {
    pub language: String,
    pub openai_api_key: String,
    pub audio_output_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: "English".to_string(),
            openai_api_key: String::new(),
            audio_output_enabled: true,
        }
    }
}

pub fn load_settings() -> Result<Settings, SharadError> {
    if let Ok(metadata) = fs::metadata(SETTINGS_FILE) {
        if metadata.is_file() {
            let data = fs::read_to_string(SETTINGS_FILE)?;
            let settings: Settings = serde_json::from_str(&data)?;
            return Ok(settings);
        }
    }
    Ok(Settings::default())
}

pub fn save_settings(settings: &Settings) -> Result<(), SharadError> {
    let json = serde_json::to_string_pretty(settings)?;
    fs::write(SETTINGS_FILE, json)?;
    Ok(())
}
