mod assistant;
mod audio;
mod image;
mod settings;
mod utils;

use crate::assistant::{
    load_conversation_from_file, run_conversation, run_conversation_with_save, run_test,
};
use crate::settings::{load_settings, save_settings, Settings};
use async_openai::Client;
use chrono::Local;
use colored::*;
use core::cmp::Ordering;
use rpassword::read_password;
use self_update::backends::github::{ReleaseList, Update};
use semver::Version;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io;
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
        println!("Newest version found: {}", release.version);

        let latest_version = Version::parse(&release.version)?;
        let current_version = Version::parse(current_version)?;

        match latest_version.cmp(&current_version) {
            Ordering::Greater => {
                println!("Updating to new version: {}", release.version);

                // Perform the update
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
            Ordering::Equal => {
                println!("Current version is up to date.");
            }
            Ordering::Less => {
                println!("You're in the future.");
            }
        }
    } else {
        println!("No new updates found.");
    }

    Ok(())
}

async fn validate_openai_key(settings: &mut Settings) -> Result<(), Box<dyn std::error::Error>> {
    while settings.openai_api_key.is_empty() || !is_valid_key(&settings.openai_api_key).await {
        eprintln!("{}", "Invalid API Key".red());
        print!("Enter your OpenAI API Key: ");
        io::stdout().flush()?; // Ensure the prompt is displayed immediately
        let api_key = read_password()?;
        settings.openai_api_key = api_key;

        if is_valid_key(&settings.openai_api_key).await {
            io::stdout().flush()?; // Ensure the prompt is displayed immediately
            let _ = save_settings(settings);
            break;
        } else {
            eprintln!("{}", "Invalid API Key".red());
            io::stdout().flush()?; // Ensure the prompt is displayed immediately
            settings.openai_api_key.clear(); // Clear the invalid API key
        }
    }
    println!("{}", "API Key is valid.".green());
    io::stdout().flush()?; // Ensure the prompt is displayed immediately
    Ok(())
}

async fn is_valid_key(api_key: &str) -> bool {
    env::set_var("OPENAI_API_KEY", api_key);
    let client = Client::new();

    client.models().list().await.is_ok()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check for updates in a blocking context
    let update_result = tokio::task::spawn_blocking(check_for_updates).await?;
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
    "#;

    let welcome = "Welcome to Sharad v";
    let version = env!("CARGO_PKG_VERSION");
    let intro = "You can quit at any time by saying \"exit\".";

    println!("{:^80}", art.green());
    println!("{:^80}", format!("{}{}", welcome.green(), version.cyan()));
    println!("{:^80}", intro.yellow());

    fs::create_dir_all("./data/logs")?;
    let log_file_path = format!(
        "./data/logs/log_{}.txt",
        Local::now().format("%Y%m%d_%H%M%S")
    );
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
    validate_openai_key(&mut settings).await?;

    loop {
        println!("Main Menu");
        println!("1. Start a new game");
        println!("2. Load a game");
        println!("3. Create an image");
        println!("4. Settings");
        println!("9. Test");
        println!("0. Exit");

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
                println!("{}", "No save files found.".red());
            }
            "3" => {
                let prompt = utils::get_user_input("What image would you like to generate? ");
                let _ = image::generate_and_save_image(&prompt).await;
            }
            "4" => {
                if let Err(e) = change_settings(&mut settings).await {
                    eprintln!("Failed to change settings: {}", e);
                    return Err(e);
                }
            }
            "9" => {
                println!("What did you excpect!?");
                run_test(&mut log_file).await?;
            }
            "0" => {
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

async fn change_settings(
    settings: &mut Settings,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Settings Menu");
    println!("1. Change Language. Language = {}", settings.language);
    println!("2. Change OpenAI API Key");
    println!(
        "3. Audio input and Output. {}",
        settings.audio_output_enabled
    );
    println!("0. Back to Main Menu");

    let choice = utils::get_user_input("Enter your choice: ");

    match choice.trim() {
        "1" => {
            let choice = utils::get_user_input("Enter the language you want to play in: ");
            settings.language = choice.to_string();
            println!("Language changed to {}.", settings.language);
        }
        "2" => {
            settings.openai_api_key.clear(); // Clear the invalid API key
            print!("Enter your OpenAI API Key: ");
            io::stdout().flush()?; // Ensure the prompt is displayed immediately
            let api_key = read_password()?;
            settings.openai_api_key = api_key;
            let _ = validate_openai_key(settings).await;
        }
        "3" => {
            settings.audio_output_enabled = !settings.audio_output_enabled;
            println!(
                "Audio Output is now {}.",
                if settings.audio_output_enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            );
        }
        "0" => return Ok(()),
        _ => println!("Invalid choice. Please enter a valid number."),
    }

    save_settings(settings)?;

    Ok(())
}
