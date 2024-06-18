use crate::audio::{generate_and_play_audio, record_and_transcribe_audio};
use crate::settings::load_settings;
use crate::utils::correct_input;
use async_openai::types::ListAssistantsResponse;
use async_openai::{
    types::{
        CreateMessageRequestArgs, CreateRunRequestArgs, CreateThreadRequestArgs, MessageContent,
        MessageRole, RunStatus,
    },
    Audio, Client,
};
use colored::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::io::Write;
use std::{error::Error, fs::File};
use tokio::time::Duration;

const SAVE_DIR: &str = "./logs/saves/";

#[derive(Serialize)]
struct ListAssistantsQuery {
    // Add fields as needed for your query, for now, we'll use an empty struct
}

#[derive(Serialize, Deserialize)]
pub struct Save {
    pub assistant_id: String,
    pub thread_id: String,
}

pub fn save_conversation(assistant_id: &str, thread_id: &str) -> Result<(), Box<dyn Error>> {
    let save_name = crate::utils::get_user_input("Enter a name for the save file: ");
    let save_file = format!("{}{}.json", SAVE_DIR, save_name);
    let save = Save {
        assistant_id: assistant_id.to_string(),
        thread_id: thread_id.to_string(),
    };
    let json = serde_json::to_string(&save)?;
    fs::create_dir_all(SAVE_DIR)?; // Create the directory if it doesn't exist
    let mut file = fs::File::create(save_file)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

pub fn load_conversation_from_file() -> Result<Save, Box<dyn Error>> {
    let save_files: Vec<_> = fs::read_dir(SAVE_DIR)?
        .filter_map(|entry| {
            entry.ok().and_then(|e| {
                let path = e.path();
                if path.is_file()
                    && path.extension().and_then(|os_str| os_str.to_str()) == Some("json")
                {
                    path.file_stem()
                        .and_then(|os_str| os_str.to_str().map(|s| s.to_string()))
                } else {
                    None
                }
            })
        })
        .collect();

    if save_files.is_empty() {
        return Err("No save files found.".into());
    }

    println!("Available save files:");
    for (index, save_file) in save_files.iter().enumerate() {
        println!("{}. {}", index + 1, save_file);
    }

    let choice = loop {
        let input = crate::utils::get_user_input("Enter the number of the save file to load: ");
        match input.trim().parse::<usize>() {
            Ok(num) if num > 0 && num <= save_files.len() => break num,
            _ => println!("Invalid choice. Please enter a valid number."),
        }
    };

    let save_file = format!("{}{}.json", SAVE_DIR, save_files[choice - 1]);
    let data = fs::read_to_string(save_file)?;
    let save: Save = serde_json::from_str(&data)?;
    Ok(save)
}

fn choose_assistant(assistants: Vec<(String, String)>) -> Result<String, Box<dyn Error>> {
    println!("Available Game cartridges:");
    for (i, (_, name)) in assistants.iter().enumerate() {
        println!("{}: {}", i + 1, name);
    }

    print!("Choose a game cartridge by number: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let choice: usize = input.trim().parse()?;

    if choice == 0 || choice > assistants.len() {
        return Err("Invalid choice".into());
    }

    Ok(assistants[choice - 1].0.clone())
}

pub async fn list_assistants() -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let client = Client::new();
    let query = ListAssistantsQuery {
        // Initialize any required query parameters here
    };
    let response: ListAssistantsResponse = client.assistants().list(&query).await?;

    let assistants = response
        .data
        .into_iter()
        .map(|assistant| (assistant.id, assistant.name.unwrap_or_default()))
        .collect();

    Ok(assistants)
}

pub async fn run_conversation(
    log_file: &mut File,
    is_new_game: bool,
) -> Result<(), Box<dyn Error>> {
    let (assistant_id, thread_id) = if is_new_game {
        let assistants = list_assistants().await?;
        if assistants.is_empty() {
            println!("No game cartridge available.");
            return Ok(());
        }

        let assistant_id = choose_assistant(assistants)?;

        let client = Client::new();
        let thread = client
            .threads()
            .create(CreateThreadRequestArgs::default().build()?)
            .await?;
        save_conversation(&assistant_id, &thread.id)?;
        (assistant_id, thread.id)
    } else {
        let save = load_conversation_from_file()?;
        (save.assistant_id, save.thread_id)
    };

    run_conversation_with_save(log_file, assistant_id, thread_id).await
}

pub async fn run_conversation_with_save(
    log_file: &mut File,
    assistant_id: String,
    thread_id: String,
) -> Result<(), Box<dyn Error>> {
    let client = Client::new();
    let audio = Audio::new(&client);
    let settings = load_settings().unwrap_or_default();

    let initial_message = CreateMessageRequestArgs::default()
        .role(MessageRole::Assistant)
        .content(format!("Welcome the player to the world and ask them who they are or want to be. Only write in the following language: {}",settings.language))
        .build()?;
    client
        .threads()
        .messages(&thread_id)
        .create(initial_message.clone())
        .await?;
    let run_request = CreateRunRequestArgs::default()
        .assistant_id(&assistant_id)
        .parallel_tool_calls(false)
        .build()?;
    let run = client
        .threads()
        .runs(&thread_id)
        .create(run_request)
        .await?;

    while client
        .threads()
        .runs(&thread_id)
        .retrieve(&run.id)
        .await?
        .status
        == RunStatus::InProgress
    {
        print!("-");
        std::io::stdout().flush()?;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let response = client
        .threads()
        .messages(&thread_id)
        .list(&[] as &[(&str, &str)])
        .await?;
    let text = match response.data.first().unwrap().content.first().unwrap() {
        MessageContent::Text(text) => text.text.value.clone(),
        _ => panic!("Unsupported content type"),
    };
    writeln!(log_file, "Assistant's response: {}", text)?;

    println!(" {}", text.green());
    generate_and_play_audio(&audio, &text, "narrator").await?;

    writeln!(log_file, "Conversation started.")?;

    loop {
        print!("🎤");
        let user_input = record_and_transcribe_audio().await?;
        let corrected_input = correct_input(&user_input)?;
        if corrected_input.trim().is_empty() {
            println!("Input cannot be empty. Please try again.");
            continue;
        }

        if corrected_input.trim().eq_ignore_ascii_case("exit") {
            break;
        }

        writeln!(log_file, "\nUser input: {}", corrected_input)?;

        println!(" {}", corrected_input);
        client
            .threads()
            .messages(&thread_id)
            .create(
                CreateMessageRequestArgs::default()
                    .role(MessageRole::User)
                    .content(corrected_input.clone())
                    .build()?,
            )
            .await?;

        let run_request = CreateRunRequestArgs::default()
            .assistant_id(&assistant_id)
            .parallel_tool_calls(false)
            .build()?;
        let run = client
            .threads()
            .runs(&thread_id)
            .create(run_request)
            .await?;

        while client
            .threads()
            .runs(&thread_id)
            .retrieve(&run.id)
            .await?
            .status
            == RunStatus::InProgress
        {
            print!("-");
            std::io::stdout().flush()?;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        let response = client
            .threads()
            .messages(&thread_id)
            .list(&[] as &[(&str, &str)])
            .await?;
        let text = match response.data.first().unwrap().content.first().unwrap() {
            MessageContent::Text(text) => text.text.value.clone(),
            _ => panic!("Unsupported content type"),
        };
        writeln!(log_file, "Assistant's response: {}", text)?;

        println!(" {}", text.green());
        generate_and_play_audio(&audio, &text, "narrator").await?;
    }

    writeln!(log_file, "Conversation ended.")?;
    log_file.sync_all()?;
    Ok(())
}
