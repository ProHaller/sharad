use crate::display::Display;

use crate::error::SharadError;
use crate::Color;
use async_openai::Client;
use crossterm::{
    cursor, execute,
    terminal::{Clear, ClearType},
};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::stdout;

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

pub async fn change_settings(
    settings: &mut Settings,
    display: &Display,
    art: &str,
) -> Result<(), SharadError> {
    loop {
        execute!(stdout(), Clear(ClearType::All), cursor::MoveTo(0, 0))?;
        display.print_centered(art, Color::Green);
        display.print_centered(
            &format!("Welcome to Sharad v{}", env!("CARGO_PKG_VERSION")),
            Color::Cyan,
        );
        display.print_centered("You can quit by inputing \"exit\".", Color::Yellow);
        display.print_separator(Color::Blue);

        display.print_centered("Settings Menu", Color::Green);
        display.print_wrapped(
            &format!("1. Change Language. Current: {}", settings.language),
            Color::White,
        );
        display.print_wrapped("2. Change OpenAI API Key", Color::White);
        display.print_wrapped(
            &format!("3. Audio Output Enabled: {}", settings.audio_output_enabled),
            Color::White,
        );
        display.print_wrapped(
            &format!("4. Audio Input Enabled: {}", settings.audio_input_enabled),
            Color::White,
        );
        display.print_wrapped(
            &format!("5. Debug Mode: {}", settings.debug_mode),
            Color::White,
        );
        display.print_wrapped("0. Back to Main Menu", Color::White);

        let choice = display.get_user_input("Enter your choice:");

        match choice.trim() {
            "1" => {
                let new_language =
                    display.get_user_input("Enter the language you want to play in:");
                if !new_language.trim().is_empty() {
                    settings.language = new_language;
                    display.print_wrapped(
                        &format!("Language changed to {}.", settings.language),
                        Color::Green,
                    );
                } else {
                    display
                        .print_wrapped("Language cannot be empty. Please try again.", Color::Red);
                }
            }
            "2" => {
                settings.openai_api_key.clear();
                validate_settings(settings, display).await?;
                // Handle API key change
            }
            "3" => {
                settings.audio_output_enabled = !settings.audio_output_enabled;
            }
            "4" => {
                settings.audio_input_enabled = !settings.audio_input_enabled;
            }
            "5" => {
                settings.debug_mode = !settings.debug_mode;
            }
            "0" => {
                break;
            }
            _ => {
                display.print_wrapped("Invalid choice. Please enter a valid number.", Color::Red);
                display.get_user_input("Press Enter to continue...");
            }
        }
    }
    save_settings(settings)?;

    Ok(())
}
