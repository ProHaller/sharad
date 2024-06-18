mod assistant;
mod audio;
mod image;
mod settings;
mod utils;

use crate::assistant::{load_conversation_from_file, run_conversation, run_conversation_with_save};
use crate::settings::{load_settings, save_settings, Settings};
use chrono::Local;
use colored::*;
use rpassword::read_password;
use self_update::backends::github::{ReleaseList, Update};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use tokio::signal;

fn check_for_updates() -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("Checking for updates...");

    // Define the repository and the binary name
    let repo_owner = "ProHaller";
    let repo_name = "sharad";
    let binary_name = "sharad";
    let current_version = env!("CARGO_PKG_VERSION");

    // Fetch the latest release info from GitHub
    let releases = ReleaseList::configure()
        .repo_owner(repo_owner)
        .repo_name(repo_name)
        .build()?
        .fetch()?;

    // Find the latest release and get the download URL
    if let Some(release) = releases.first() {
        println!("New version found: {}", release.version);

        // Perform the update
        Update::configure()
            .repo_owner(repo_owner)
            .repo_name(repo_name)
            .bin_name(binary_name)
            .target(self_update::get_target())
            .show_download_progress(true)
            .show_output(true)
            .bin_install_path(std::env::current_exe()?.parent().unwrap())
            .current_version(current_version)
            .target_version_tag(release.version.as_str())
            .build()?
            .update()?;
    } else {
        println!("No new updates found.");
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check for updates in a blocking context
    let update_result = tokio::task::spawn_blocking(|| check_for_updates()).await?;
    if let Err(e) = update_result {
        eprintln!("Failed to check for updates: {}", e);
    }

    let art = r#"
        _____   .                 A            .              .   .       .
        o o o\            .     _/_\_                                  |\
       ------\\      .       __//...\\__                .              ||\   .
       __ A . |\         .  <----------→     .                  .      ||||
     HH|\. .|||                \\\|///                 ___|_           ||||
     ||| | . \\\     A    .      |.|                  /|  .|    .      /||\
       | | .  |||   / \          |.|     .           | | ..|          /.||.\
     ..| | . . \\\ ||**|         |.|   _A_     ___   | | ..|         || |\ .|
     ..| | , ,  |||||**|         |.|  /| |   /|   |  |.| ..|         || |*|*|
     ..|.| . . . \\\|**|.  ____  |.| | | |  | |***|  |.| ..|  _____  || |*|\|\
     ..|.| . . .  |||**| /|.. .| |.| |*|*|  | |*  | ___| ..|/|  .  | || |*| |\\
     -----------,. \\\*|| |.. .|//|\\|*|*_____| **||| ||  .| | ..  |/|| |*| |\\
     Sharad game \  ||||| |..  // A \\*/| . ..| * ||| || ..| |  .  ||||,|*| | \
      By Roland  |\. \\\| |.. // /|\ \\ | . ..|** ||| || ..| | . . ||||.|*| |\\
       and the    \\  ||| |, ||.| | | ||| . ..| * ||| ||  .| | ..  ||||.|*| ||||
     Haller Family || ||| |, ||.| | | ||| . ..| * ||| || ..| | . ..||||.|*| ||||
     ---------------------------------------------------------------------------

                      _____ _                         _
                     / ____| |                       | |
                    | (___ | |__   __ _ _ __ __ _  __| |
                     \___ \| '_ \ / _` | '__/ _` |/ _` |
                     ____) | | | | (_| | | | (_| | (_| |
                    |_____/|_| |_|\__,_|_|  \__,_|\__,_|

                            Welcome to Sharad! (v 0.5.2)
    "#;

    let intro =
        "You can quit at any time by typing \"exit\". Be aware, there is no save in Sharad.";

    println!("{}", art.green());
    println!("{}", intro.yellow());

    fs::create_dir_all("./logs")?;
    let log_file_path = format!("./logs/log_{}.txt", Local::now().format("%Y%m%d_%H%M%S"));
    let mut log_file = match File::create(&log_file_path) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("Failed to create log file: {}", e);
            return Err(e.into());
        }
    };

    // Handle SIGINT (Ctrl-C)
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl-C");
        std::process::exit(0);
    });

    writeln!(log_file, "Sharad game started.")?;

    let mut settings = match load_settings() {
        Ok(settings) => settings,
        Err(e) => {
            eprintln!("Failed to load settings: {}", e);
            Settings::default()
        }
    };

    // Set the OpenAI API key from settings
    if !settings.openai_api_key.is_empty() {
        env::set_var("OPENAI_API_KEY", &settings.openai_api_key);
    } else {
        println!("No OpenAI API key found in settings. Please set it in settings");
    }

    loop {
        println!("Main Menu");
        println!("1. Start a new game");
        println!("2. Load a game");
        println!("3. Settings");
        println!("4. Exit");

        let choice = utils::get_user_input("Enter your choice: ");

        match choice.trim() {
            "1" => {
                writeln!(log_file, "Starting a new game.")?;
                if let Err(e) = run_conversation(&mut log_file, true).await {
                    eprintln!("Failed to run conversation: {}", e);
                    return Err(e);
                }
            }
            "2" => {
                writeln!(log_file, "Loading a game.")?;
                if let Ok(save) = load_conversation_from_file() {
                    if let Err(e) =
                        run_conversation_with_save(&mut log_file, save.assistant_id, save.thread_id)
                            .await
                    {
                        eprintln!("Failed to run conversation: {}", e);
                        return Err(e);
                    }
                }
            }
            "3" => {
                if let Err(e) = change_settings(&mut settings) {
                    eprintln!("Failed to change settings: {}", e);
                    return Err(e);
                }
            }
            "4" => {
                writeln!(log_file, "Exiting game.")?;
                break;
            }
            _ => println!("Invalid choice. Please enter a valid number."),
        }
    }

    if let Err(e) = log_file.sync_all() {
        eprintln!("Failed to sync log file: {}", e);
        return Err(e.into());
    }

    writeln!(log_file, "Sharad game ended.")?;
    Ok(())
}

fn change_settings(
    settings: &mut Settings,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Settings Menu");
    println!("1. Change Language");
    println!("2. Set OpenAI API Key");
    println!("0. Back to Main Menu");

    let choice = utils::get_user_input("Enter your choice: ");

    match choice.trim() {
        "1" => {
            let choice = utils::get_user_input("Enter the language you want to play in: ");
            settings.language = choice.to_string();
            println!("Language changed to {}.", settings.language);
        }
        "2" => {
            println!("Enter your OpenAI API Key: ");
            let api_key = read_password()?;
            settings.openai_api_key = api_key;
            println!("OpenAI API Key updated.");
            // Update the environment variable
            env::set_var("OPENAI_API_KEY", &settings.openai_api_key);
        }
        "0" => return Ok(()),
        _ => println!("Invalid choice. Please enter a valid number."),
    }

    save_settings(settings)?;

    Ok(())
}
