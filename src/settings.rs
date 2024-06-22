use crate::display::Display;
use crate::error::SharadError;
use crate::Color;
use serde::{Deserialize, Serialize};
use std::fs;

const SETTINGS_FILE: &str = "./data/logs/settings.json";

#[derive(Serialize, Deserialize)]
pub struct Settings {
    pub language: String,
    pub openai_api_key: String,
    pub audio_output_enabled: bool,
    pub audio_input_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: "Français".to_string(),
            openai_api_key: String::new(),
            audio_output_enabled: true,
            audio_input_enabled: true,
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

pub fn load_individual_setting<T>(key: &str) -> Result<T, SharadError>
where
    T: for<'de> Deserialize<'de> + Default,
{
    if let Ok(metadata) = fs::metadata(SETTINGS_FILE) {
        if metadata.is_file() {
            let data = fs::read_to_string(SETTINGS_FILE)?;
            let settings: serde_json::Value = serde_json::from_str(&data)?;
            if let Some(value) = settings.get(key) {
                return serde_json::from_value(value.clone()).map_err(SharadError::SerdeJson);
            }
        }
    }
    Ok(T::default())
}

pub fn validate_setting<T>(value: &T, default: &T) -> Result<(), SharadError>
where
    T: PartialEq + Default,
{
    if value == default {
        Err(SharadError::Other("Invalid setting".to_string()))
    } else {
        Ok(())
    }
}

pub fn load_and_validate_setting<T>(key: &str, default: T, display: &Display) -> T
where
    T: for<'de> Deserialize<'de> + Default + PartialEq,
{
    match load_individual_setting::<T>(key) {
        Ok(value) => {
            if validate_setting(&value, &default).is_err() {
                display.print_wrapped(
                    &format!("Invalid {} setting. Resetting to default.", key),
                    Color::Red,
                );
                default
            } else {
                value
            }
        }
        Err(e) => {
            display.print_wrapped(
                &format!("Failed to load {} setting: {}", key, e),
                Color::Red,
            );
            default
        }
    }
}
pub fn save_settings(settings: &Settings) -> Result<(), SharadError> {
    let json = serde_json::to_string_pretty(settings)?;
    fs::write(SETTINGS_FILE, json)?;
    Ok(())
}
