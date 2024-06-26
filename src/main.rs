mod assistant;
mod audio;
mod display;
mod error;
mod image;
mod menu;
mod settings;
mod utils;

use crate::display::Display;
use crate::error::SharadError;
use chrono::Local;
use colored::*;
use menu::main_menu;
use std::fs::{self, File};
use std::io::Write;

use core::cmp::Ordering;
use rand::Rng;
use self_update::backends::github::{ReleaseList, Update};
use semver::Version;
use std::env;
use std::error::Error;
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
            Ordering::Equal => println!("{}", "Current version is up to date.".green()),
            Ordering::Less => rainbow("You're in the future."),
        }
    } else {
        println!("No new updates found.");
    }

    println!();
    let display = Display::new();
    display.get_user_input("press enter to continue...");
    Ok(())
}

fn rainbow(text: &str) {
    let colors = [
        Color::Red,
        Color::Yellow,
        Color::Green,
        Color::Cyan,
        Color::Blue,
        Color::Magenta,
    ];
    let mut rng = rand::thread_rng();

    for c in text.chars() {
        let color = colors[rng.gen_range(0..colors.len())];
        print!("{}", c.to_string().color(color).bold());
    }
}

#[tokio::main]
async fn main() -> Result<(), SharadError> {
    let display = Display::new();

    let update_result = tokio::task::spawn_blocking(check_for_updates).await?;
    if let Err(e) = update_result {
        display.print_wrapped(&format!("Failed to check for updates: {}", e), Color::Red);
    }

    fs::create_dir_all("./data/logs")?;
    let log_file_path = format!("./data/logs/log_{}.txt", Local::now().format("%Y%m%d_%H"));
    let mut log_file = File::create(&log_file_path).map_err(|e| {
        display.print_wrapped(&format!("Failed to create log file: {}", e), Color::Red);
        SharadError::Io(e)
    })?;

    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl-C");
        std::process::exit(0);
    });

    writeln!(log_file, "Sharad game started.")?;

    let _ = main_menu(log_file).await;
    // Display the art once before entering the loop
    Ok(())
}
