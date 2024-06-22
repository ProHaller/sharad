use crate::audio::{generate_and_play_audio, record_and_transcribe_audio};
use crate::display::Display;
use crate::error::SharadError;
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
use std::fs::{self, File};
use std::io::Write;
use tokio::time::Duration;

const SAVE_DIR: &str = "./data/logs/saves/";

#[derive(Serialize)]
struct ListAssistantsQuery {}

#[derive(Serialize, Deserialize)]
pub struct Save {
    pub assistant_id: String,
    pub thread_id: String,
}

pub fn save_conversation(
    assistant_id: &str,
    thread_id: &str,
    display: &Display,
) -> Result<(), SharadError> {
    let save_name = display.get_user_input("Enter a name for the save file:");
    let save_file = format!("{}{}.json", SAVE_DIR, save_name);
    let save = Save {
        assistant_id: assistant_id.to_string(),
        thread_id: thread_id.to_string(),
    };
    let json = serde_json::to_string(&save)?;
    fs::create_dir_all(SAVE_DIR)?;
    let mut file = File::create(save_file)?;
    file.write_all(json.as_bytes())?;
    display.print_wrapped("Game saved successfully.", Color::Green);
    Ok(())
}

pub fn load_conversation_from_file(display: &Display) -> Result<Save, SharadError> {
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
        return Err(SharadError::Other("No save files found.".into()));
    }

    display.print_wrapped("Input '0' to go back to main menu.", Color::Yellow);
    display.print_wrapped("Available save files:", Color::Yellow);
    for (index, save_file) in save_files.iter().enumerate() {
        display.print_wrapped(&format!("{}. {}", index + 1, save_file), Color::White);
    }

    let choice = loop {
        let input = display.get_user_input("Enter the number of the save file to load:");
        match input.trim().parse::<usize>() {
            Ok(num) if num > 0 && num <= save_files.len() => break num,
            Ok(0) => return Err(SharadError::Message(String::from("Back to main menu."))),
            _ => display.print_wrapped("Invalid choice. Please enter a valid number.", Color::Red),
        }
    };

    let save_file = format!("{}{}.json", SAVE_DIR, save_files[choice - 1]);
    let data = fs::read_to_string(save_file)?;
    let save: Save = serde_json::from_str(&data)?;
    Ok(save)
}

async fn choose_assistant(
    assistants: Vec<(String, String)>,
    display: &Display,
) -> Result<String, SharadError> {
    display.print_wrapped("Available Game cartridges:", Color::Yellow);
    for (i, (_, name)) in assistants.iter().enumerate() {
        display.print_wrapped(&format!("{}. {}", i + 1, name), Color::White);
    }

    loop {
        let input = display.get_user_input("Choose a game cartridge by number:");
        match input.trim().parse::<usize>() {
            Ok(num) if num > 0 && num <= assistants.len() => {
                return Ok(assistants[num - 1].0.clone())
            }
            _ => display.print_wrapped("Invalid choice. Please enter a valid number.", Color::Red),
        }
    }
}

pub async fn list_assistants() -> Result<Vec<(String, String)>, SharadError> {
    let client = Client::new();
    let query = ListAssistantsQuery {};
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
    display: &Display,
) -> Result<(), SharadError> {
    let client = Client::new();
    let language = load_settings()?.language;
    let (assistant_id, thread_id) = if is_new_game {
        let assistants = list_assistants().await?;
        if assistants.is_empty() {
            display.print_wrapped("No game cartridge available.", Color::Red);
            return Ok(());
        }

        let assistant_id = choose_assistant(assistants, display).await?;

        let thread = client
            .threads()
            .create(CreateThreadRequestArgs::default().build()?)
            .await?;

        // For a new game, send an initial message
        let initial_message = CreateMessageRequestArgs::default()
            .role(MessageRole::User)
            .content(format!("Welcome the player to the world and ask them who they are or want to be. Always write in the following language: {}", language))
            .build()?;
        client
            .threads()
            .messages(&thread.id)
            .create(initial_message)
            .await?;

        save_conversation(&assistant_id, &thread.id, display)?;
        (assistant_id, thread.id)
    } else {
        let save = load_conversation_from_file(display)?;
        (save.assistant_id, save.thread_id)
    };

    run_conversation_with_save(log_file, assistant_id, thread_id, is_new_game, display).await
}

pub async fn run_conversation_with_save(
    log_file: &mut File,
    assistant_id: String,
    thread_id: String,
    is_new_game: bool,
    display: &Display,
) -> Result<(), SharadError> {
    let client = Client::new();
    let audio = Audio::new(&client);

    if is_new_game {
        display.print_header("Welcome to the Adventure");

        // For a new game, we'll retrieve the initial message from the assistant
        let run_request = CreateRunRequestArgs::default()
            .assistant_id(&assistant_id)
            .parallel_tool_calls(false)
            .build()?;
        let run = client
            .threads()
            .runs(&thread_id)
            .create(run_request)
            .await?;

        // Wait for the run to complete
        display.print_thinking();
        loop {
            let run_status = client.threads().runs(&thread_id).retrieve(&run.id).await?;

            if run_status.status == RunStatus::Completed {
                break;
            } else if run_status.status == RunStatus::Failed {
                return Err(SharadError::Other("Run failed".to_string()));
            }

            display.print_thinking_dot();
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        display.clear_thinking();

        // Retrieve the initial message
        let messages = client
            .threads()
            .messages(&thread_id)
            .list(&[("limit", "1")])
            .await?;

        if let Some(latest_message) = messages.data.first() {
            if let Some(MessageContent::Text(text_content)) = latest_message.content.first() {
                let response_text = &text_content.text.value;
                writeln!(log_file, "Assistant's initial message: {}", response_text)?;
                display.print_separator(Color::Cyan);
                display.print_wrapped(response_text, Color::Green);
                display.print_separator(Color::Cyan);
                generate_and_play_audio(&audio, response_text, "narrator").await?;
            }
        }
    } else {
        display.print_header("Welcome back to the Adventure");

        // Retrieve all messages from the thread
        let mut all_messages = Vec::new();
        let mut before: Option<String> = None;
        loop {
            let mut params = vec![("order", "desc"), ("limit", "100")];
            if let Some(before_id) = &before {
                params.push(("before", before_id));
            }
            let messages = client.threads().messages(&thread_id).list(&params).await?;
            all_messages.extend(messages.data.into_iter().rev());
            if messages.has_more {
                before = messages.first_id;
            } else {
                break;
            }
        }

        // Display all messages
        display.print_wrapped("Previous conversation:", Color::Yellow);
        for message in &all_messages {
            let role = match message.role {
                MessageRole::User => "You",
                MessageRole::Assistant => "Game",
            };
            display.print_separator(Color::Cyan);
            display.print_wrapped(&format!("{}: ", role), Color::Yellow);
            if let Some(MessageContent::Text(text_content)) = message.content.first() {
                display.print_wrapped(
                    &text_content.text.value,
                    if role == "You" {
                        Color::Blue
                    } else {
                        Color::Green
                    },
                );
            }
        }
        display.print_separator(Color::Cyan);
        display.print_wrapped("End of previous conversation.", Color::Yellow);
    }

    // Main conversation loop
    loop {
        // Get user input
        let user_input = record_and_transcribe_audio(display).await?;
        let corrected_input = correct_input(display, &user_input)?;
        if corrected_input.trim().is_empty() {
            display.print_wrapped("Input cannot be empty. Please try again.", Color::Red);
            continue;
        }

        if corrected_input.trim().eq_ignore_ascii_case("exit") {
            break;
        }

        writeln!(log_file, "\nUser input: {}", corrected_input)?;

        display.print_separator(Color::Magenta);
        display.print_wrapped(&corrected_input, Color::Blue);
        display.print_separator(Color::Magenta);

        // Send user message
        client
            .threads()
            .messages(&thread_id)
            .create(
                CreateMessageRequestArgs::default()
                    .role(MessageRole::User)
                    .content(corrected_input)
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

        // Wait for the run to complete
        display.print_thinking();
        loop {
            let run_status = client.threads().runs(&thread_id).retrieve(&run.id).await?;

            if run_status.status == RunStatus::Completed {
                break;
            } else if run_status.status == RunStatus::Failed {
                return Err(SharadError::Other("Run failed".to_string()));
            }

            display.print_thinking_dot();
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        display.clear_thinking();

        // Retrieve only the latest message
        let messages = client
            .threads()
            .messages(&thread_id)
            .list(&[("limit", "1")])
            .await?;

        if let Some(latest_message) = messages.data.first() {
            if let Some(MessageContent::Text(text_content)) = latest_message.content.first() {
                let response_text = &text_content.text.value;
                writeln!(log_file, "Assistant's response: {}", response_text)?;
                display.print_separator(Color::Cyan);
                display.print_wrapped(response_text, Color::Green);
                display.print_separator(Color::Cyan);
                generate_and_play_audio(&audio, response_text, "narrator").await?;
            }
        }
    }

    display.print_footer("Thank you for playing!");

    writeln!(log_file, "Conversation ended.")?;
    log_file.sync_all()?;
    Ok(())
}
