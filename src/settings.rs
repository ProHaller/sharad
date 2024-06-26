use crate::display::Display;

use crate::error::SharadError;
use crate::Color;
use async_openai::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;

const SETTINGS_FILE: &str = "./data/logs/settings.json";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Settings {
    pub language: String,
    pub openai_api_key: String,
    #[serde(default = "default_true")]
    pub audio_output_enabled: bool,
    #[serde(default = "default_true")]
    pub audio_input_enabled: bool,
    #[serde(default)]
    pub debug_mode: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: "Français".to_string(),
            openai_api_key: String::new(),
            audio_output_enabled: true,
            audio_input_enabled: true,
            debug_mode: false,
        }
    }
}

pub fn load_settings() -> Result<Settings, SharadError> {
    match fs::read_to_string(SETTINGS_FILE) {
        Ok(data) => {
            let mut settings: Settings =
                serde_json::from_str(&data).unwrap_or_else(|_| Settings::default());

            // Check for empty required fields and set default values
            if settings.language.trim().is_empty() {
                settings.language = Settings::default().language;
            }
            if settings.openai_api_key.trim().is_empty() {
                settings.openai_api_key = Settings::default().openai_api_key;
            }

            Ok(settings)
        }
        Err(_) => Ok(Settings::default()),
    }
}

pub fn save_settings(settings: &Settings) -> Result<(), SharadError> {
    let json = serde_json::to_string_pretty(settings)?;
    fs::write(SETTINGS_FILE, json)?;
    Ok(())
}

pub async fn validate_settings(
    settings: &mut Settings,
    display: &Display,
) -> Result<(), SharadError> {
    // Validate OpenAI API Key
    while !is_valid_key(&settings.openai_api_key).await {
        display.print_wrapped("Invalid or empty API Key", Color::Red);
        settings.openai_api_key = display.get_user_input("Enter your OpenAI API Key:");
    }
    display.print_wrapped("API Key is valid.", Color::Green);

    // Ensure language is not empty
    if settings.language.trim().is_empty() {
        settings.language = Settings::default().language;
        display.print_wrapped(
            &format!("Language was empty. Set to default: {}", settings.language),
            Color::Yellow,
        );
    }

    save_settings(settings)?;
    Ok(())
}

async fn is_valid_key(api_key: &str) -> bool {
    if api_key.is_empty() {
        return false;
    }
    env::set_var("OPENAI_API_KEY", api_key);
    let client = Client::new();
    client.models().list().await.is_ok()
}
