mod assistant;
mod audio;
mod display;
mod error;
mod image;
mod settings;
mod utils;

use crate::assistant::{load_conversation_from_file, run_conversation, run_conversation_with_save};
use crate::settings::{load_and_validate_setting, save_settings, Settings};
use async_openai::Client;
use chrono::Local;
use colored::*;
use core::cmp::Ordering;
use display::Display;
use error::SharadError;
use self_update::backends::github::{ReleaseList, Update};
use semver::Version;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use tokio::signal;

fn check_for_updates() -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("Checking for updates...");

    let repo_owner = "ProHaller";
    let repo_name = "sharad";
    let binary_name = "sharad";
    let current_version = env!("CARGO_PKG_VERSION");

    let releases = ReleaseList::configure()
        .repo_owner(repo_owner)
        .repo_name(repo_name)
        .build()?
        .fetch()?;

    if let Some(release) = releases.first() {
        println!("Newest version found: {}", release.version);

        let latest_version = Version::parse(&release.version)?;
        let current_version = Version::parse(current_version)?;

        match latest_version.cmp(&current_version) {
            Ordering::Greater => {
                println!("Updating to new version: {}", release.version);
                Update::configure()
                    .repo_owner(repo_owner)
                    .repo_name(repo_name)
                    .bin_name(binary_name)
                    .target(self_update::get_target())
                    .show_download_progress(true)
                    .show_output(true)
                    .bin_install_path(std::env::current_exe()?.parent().unwrap())
                    .current_version(&current_version.to_string())
                    .target_version_tag(&release.version)
                    .build()?
                    .update()?;
            }
            Ordering::Equal => println!("Current version is up to date."),
            Ordering::Less => println!("You're in the future."),
        }
    } else {
        println!("No new updates found.");
    }

    Ok(())
}

async fn is_valid_key(api_key: &str) -> bool {
    env::set_var("OPENAI_API_KEY", api_key);
    let client = Client::new();
    client.models().list().await.is_ok()
}

#[tokio::main]
async fn main() -> Result<(), SharadError> {
    let display = Display::new();

    let update_result = tokio::task::spawn_blocking(check_for_updates).await?;
    if let Err(e) = update_result {
        display.print_wrapped(&format!("Failed to check for updates: {}", e), Color::Red);
    }

    let art = r#"
     ----------------------------------------------------------------------------- 
    |    _____   .                 A            .              .   .       .      |
    |    o o o\            .     _/_\_                                  |\        |
    |   ------\\      .       __//...\\__                .              ||\   .   |
    |   __ A . |\         .  <----------→     .                  .      ||||      |
    | HH|\. .|||                \\\|///                 ___|_           ||||      |
    | ||| | . \\\     A    .      |.|                  /|  .|    .      /||\      |
    |   | | .  |||   / \          |.|     .           | | ..|          /.||.\     |
    | ..| | . . \\\ ||**|         |.|   _A_     ___   | | ..|         || |\ .|    |
    | ..| | , ,  |||||**|         |.|  /| |   /|   |  |.| ..|         || |*|*|    |
    | ..|.| . . . \\\|**|.  ____  |.| | | |  | |***|  |.| ..|  _____  || |*|\|\   |
    | ..|.| . . .  |||**| /|.. .| |.| |*|*|  | |*  | ___| ..|/|  .  | || |*| |\\  |
    | -----------,. \\\*|| |.. .|//|\\|*|*_____| **||| ||  .| | ..  |/|| |*| |\\  |
    | Sharad game \  ||||| |..  // A \\*/| . ..| * ||| || ..| |  .  ||||,|*| | \  |
    |  By Roland  |\. \\\| |.. // /|\ \\ | . ..|** ||| || ..| | . . ||||.|*| |\\  |
    |   and the    \\  ||| |, ||.| | | ||| . ..| * ||| ||  .| | ..  ||||.|*| |||| |
    | Haller Family || ||| |, ||.| | | ||| . ..| * ||| || ..| | . ..||||.|*| |||| |
     ----------------------------------------------------------------------------- 

  _____ _                         _
 / ____| |                       | |
| (___ | |__   __ _ _ __ __ _  __| |
 \___ \| '_ \ / _` | '__/ _` |/ _` |
 ____) | | | | (_| | | | (_| | (_| |
|_____/|_| |_|\__,_|_|  \__,_|\__,_|
    "#;

    display.print_centered(art, Color::Green);
    display.print_centered(
        &format!("Welcome to Sharad v{}", env!("CARGO_PKG_VERSION")),
        Color::Cyan,
    );
    display.print_centered(
        "You can quit at any time by saying \"exit\".",
        Color::Yellow,
    );

    fs::create_dir_all("./data/logs")?;
    let log_file_path = format!(
        "./data/logs/log_{}.txt",
        Local::now().format("%Y%m%d_%H%M%S")
    );
    let mut log_file = File::create(&log_file_path).map_err(|e| {
        display.print_wrapped(&format!("Failed to create log file: {}", e), Color::Red);
        SharadError::Io(e)
    })?;

    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl-C");
        std::process::exit(0);
    });

    writeln!(log_file, "Sharad game started.")?;

    let mut settings = Settings {
        language: load_and_validate_setting("language", Settings::default().language, &display),
        openai_api_key: load_and_validate_setting(
            "openai_api_key",
            Settings::default().openai_api_key,
            &display,
        ),
        audio_output_enabled: load_and_validate_setting(
            "audio_output_enabled",
            Settings::default().audio_output_enabled,
            &display,
        ),
        audio_input_enabled: load_and_validate_setting(
            "audio_input_enabled",
            Settings::default().audio_input_enabled,
            &display,
        ),
    };

    validate_openai_key(&mut settings, &display).await?;

    loop {
        display.print_separator(Color::Blue);
        display.print_centered("Main Menu", Color::Green);
        display.print_wrapped("1. Start a new game", Color::White);
        display.print_wrapped("2. Load a game", Color::White);
        display.print_wrapped("3. Create an image", Color::White);
        display.print_wrapped("4. Settings", Color::White);
        display.print_wrapped("0. Exit", Color::White);

        let choice = display.get_user_input("Enter your choice:");

        match choice.trim() {
            "1" => {
                display.print_wrapped("Starting a new game.", Color::Green);
                if let Err(e) = run_conversation(&mut log_file, true, &display).await {
                    display
                        .print_wrapped(&format!("Failed to run conversation: {}", e), Color::Red);
                }
            }
            "2" => {
                display.print_wrapped("Loading a game.", Color::Green);
                match load_conversation_from_file(&display) {
                    Ok(save) => {
                        if let Err(e) = run_conversation_with_save(
                            &mut log_file,
                            save.assistant_id,
                            save.thread_id,
                            false,
                            &display,
                        )
                        .await
                        {
                            display.print_wrapped(
                                &format!("Failed to run conversation: {}", e),
                                Color::Red,
                            );
                        }
                    }
                    Err(e) => display.print_wrapped(&format!("{}", e), Color::Red),
                }
            }
            "3" => {
                let prompt = display.get_user_input("What image would you like to generate?");
                if let Err(e) = image::generate_and_save_image(&prompt).await {
                    display.print_wrapped(&format!("Failed to generate image: {}", e), Color::Red);
                }
            }
            "4" => {
                if let Err(e) = change_settings(&mut settings, &display).await {
                    display.print_wrapped(&format!("Failed to change settings: {}", e), Color::Red);
                }
            }
            "0" => {
                display.print_wrapped("Exiting game.", Color::Green);
                break;
            }
            _ => display.print_wrapped("Invalid choice. Please enter a valid number.", Color::Red),
        }
    }

    display.print_footer("Thank you for playing Sharad!");
    Ok(())
}

async fn validate_openai_key(
    settings: &mut Settings,
    display: &Display,
) -> Result<(), SharadError> {
    while settings.openai_api_key.is_empty() || !is_valid_key(&settings.openai_api_key).await {
        display.print_wrapped("Invalid API Key", Color::Red);
        let api_key = display.get_user_input("Enter your OpenAI API Key:");
        settings.openai_api_key = api_key;

        if is_valid_key(&settings.openai_api_key).await {
            save_settings(settings)?;
            break;
        } else {
            display.print_wrapped("Invalid API Key", Color::Red);
            settings.openai_api_key.clear();
        }
    }
    display.print_wrapped("API Key is valid.", Color::Green);
    Ok(())
}

async fn change_settings(settings: &mut Settings, display: &Display) -> Result<(), SharadError> {
    loop {
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
            &format!("4. Audio input Enabled: {}", settings.audio_input_enabled),
            Color::White,
        );
        display.print_wrapped("0. Back to Main Menu", Color::White);

        let choice = display.get_user_input("Enter your choice:");

        match choice.trim() {
            "1" => {
                let new_language =
                    display.get_user_input("Enter the language you want to play in:");
                settings.language = new_language;
                display.print_wrapped(
                    &format!("Language changed to {}.", settings.language),
                    Color::Green,
                );
            }
            "2" => {
                settings.openai_api_key.clear();
                validate_openai_key(settings, display).await?;
            }
            "3" => {
                settings.audio_output_enabled = !settings.audio_output_enabled;
                display.print_wrapped(
                    &format!(
                        "Audio Output is now {}.",
                        if settings.audio_output_enabled {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    ),
                    Color::Green,
                );
            }
            "4" => {
                settings.audio_input_enabled = !settings.audio_input_enabled;
                display.print_wrapped(
                    &format!(
                        "Audio Output is now {}.",
                        if settings.audio_input_enabled {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    ),
                    Color::Green,
                );
            }
            "0" => return Ok(()),
            _ => display.print_wrapped("Invalid choice. Please enter a valid number.", Color::Red),
        }

        save_settings(settings)?;
    }
}
