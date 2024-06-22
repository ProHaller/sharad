use crate::audio::{generate_and_play_audio, record_and_transcribe_audio};
use crate::display::Display;
use crate::error::SharadError;
use crate::image::generate_character_image;
use crate::settings::load_settings;
use crate::utils::correct_input;

use async_openai::{
    config::OpenAIConfig,
    types::{
        AssistantTools, AssistantToolsFunction, CreateMessageRequestArgs, CreateRunRequestArgs,
        CreateThreadRequestArgs, FunctionObject, ListAssistantsResponse, MessageContent,
        MessageObject, MessageRole, RunObject, RunStatus, SubmitToolOutputsRunRequest,
        ToolsOutputs,
    },
    Audio, Client,
};
use colored::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs::{self, File};
use std::future::Future;
use std::io::Write;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use tokio::spawn;
use tokio::sync::Mutex;
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
    let save_dir = Path::new(SAVE_DIR);

    // Check if the save directory exists
    if !save_dir.exists() {
        display.print_wrapped("No save folder found. Creating one now.", Color::Yellow);
        fs::create_dir_all(save_dir).map_err(SharadError::Io)?;
        return Err(SharadError::Message("No save files available yet.".into()));
    }

    let save_files: Vec<_> = fs::read_dir(save_dir)
        .map_err(SharadError::Io)?
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
        return Err(SharadError::Message("No save files found.".into()));
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
            Ok(0) => return Err(SharadError::Message("Back to main menu.".into())),
            _ => display.print_wrapped("Invalid choice. Please enter a valid number.", Color::Red),
        }
    };

    let save_file = save_dir.join(format!("{}.json", save_files[choice - 1]));
    let data = fs::read_to_string(save_file).map_err(SharadError::Io)?;
    let save: Save = serde_json::from_str(&data).map_err(SharadError::SerdeJson)?;
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
        display.print_debug(
            &format!("Debug: Initial message: {:?}", initial_message.content),
            Color::Magenta,
        );
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

    let _request = CreateRunRequestArgs::default()
        .assistant_id(&assistant_id)
        .tools(vec![AssistantTools::Function(AssistantToolsFunction {
            function: FunctionObject {
                name: "generate_character_image".to_string(),
                description: Some("Generate an image based on a character description".to_string()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "description": {
                            "type": "string",
                            "description": "A detailed description of the character",
                        },
                    },
                    "required": ["description"],
                })),
            },
        })])
        .build()?;

    if is_new_game {
        handle_new_game(
            &client,
            &thread_id,
            &assistant_id,
            log_file,
            display,
            &audio,
        )
        .await?;
    } else {
        display_previous_conversation(&client, &thread_id, display).await?;
    }

    main_conversation_loop(&client, thread_id, assistant_id, log_file, display, &audio).await?;

    display.print_footer("Thank you for playing!");
    writeln!(log_file, "Conversation ended.")?;
    log_file.sync_all()?;
    Ok(())
}

async fn handle_new_game(
    client: &Client<OpenAIConfig>,
    thread_id: &str,
    assistant_id: &str,
    log_file: &mut File,
    display: &Display,
    audio: &Audio<'_, OpenAIConfig>,
) -> Result<(), SharadError> {
    display.print_header("Welcome to the Adventure");

    let _run = create_and_wait_for_run(client, thread_id, assistant_id, display).await?;

    let messages = client
        .threads()
        .messages(thread_id)
        .list(&[("limit", "1")])
        .await?;

    if let Some(latest_message) = messages.data.first() {
        if let Some(MessageContent::Text(text_content)) = latest_message.content.first() {
            let response_text = &text_content.text.value;
            log_and_display_message(
                log_file,
                response_text,
                "Assistant's initial message",
                display,
            )?;
            generate_and_play_audio(audio, response_text, "narrator").await?;
        }
    }

    Ok(())
}

async fn display_previous_conversation(
    client: &Client<OpenAIConfig>,
    thread_id: &str,
    display: &Display,
) -> Result<(), SharadError> {
    display.print_header("Welcome back to the Adventure");

    let all_messages = fetch_all_messages(client, thread_id).await?;

    display.print_wrapped("Previous conversation:", Color::Yellow);
    for message in &all_messages {
        display_message(message, display);
    }
    display.print_separator(Color::Cyan);
    display.print_wrapped("End of previous conversation.", Color::Yellow);

    Ok(())
}

async fn main_conversation_loop(
    client: &Client<OpenAIConfig>,
    thread_id: String,
    assistant_id: String,
    log_file: &mut File,
    display: &Display,
    audio: &Audio<'_, OpenAIConfig>,
) -> Result<(), SharadError> {
    let pending_tool_outputs = Arc::new(Mutex::new(Vec::new()));

    loop {
        display.print_debug("Debug: Waiting for user input", Color::Magenta);
        let user_input = get_user_input(display).await?;
        if user_input.trim().eq_ignore_ascii_case("exit") {
            break;
        }

        log_and_display_message(log_file, &user_input, "User input", display)?;

        display.print_debug("Debug: Sending user message", Color::Magenta);
        send_user_message(client, &thread_id, &user_input).await?;

        // Ensure there is no active run before creating a new one
        loop {
            let runs = client
                .threads()
                .runs(&thread_id)
                .list(&[("limit", "1")])
                .await?;
            if runs.data.is_empty() || runs.data[0].status == RunStatus::Completed {
                break;
            }
            display.print_debug("Debug: Waiting for active run to complete", Color::Magenta);
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        display.print_debug("Debug: Creating and waiting for run", Color::Magenta);
        let run = create_and_wait_for_run(client, &thread_id, &assistant_id, display).await?;

        display.print_debug("Debug: Checking for required actions", Color::Magenta);
        // Handle tool calls
        if let Some(required_action) = &run.required_action {
            display.print_debug(
                &format!("Debug: Required action type: {}", required_action.r#type),
                Color::Magenta,
            );
            if required_action.r#type == "submit_tool_outputs" {
                for tool_call in &required_action.submit_tool_outputs.tool_calls {
                    display.print_debug(
                        &format!("Debug: Processing tool call: {}", tool_call.function.name),
                        Color::Magenta,
                    );
                    if tool_call.function.name == "generate_character_image" {
                        let args: serde_json::Value =
                            serde_json::from_str(&tool_call.function.arguments)?;
                        let description = args["description"].as_str().unwrap_or("").to_string();
                        let tool_call_id = tool_call.id.clone();
                        let tool_call_id_clone = tool_call.id.clone();
                        let pending_tool_outputs_clone = Arc::clone(&pending_tool_outputs);
                        let display_clone = display.clone();

                        // Spawn a new task to handle image generation
                        spawn(async move {
                            match generate_character_image(&description).await {
                                Ok(image_path) => {
                                    display_clone.print_debug(
                                        &format!("Character image generated: {}", image_path),
                                        Color::Green,
                                    );
                                    let mut outputs = pending_tool_outputs_clone.lock().await;
                                    outputs.push(ToolsOutputs {
                                        tool_call_id: Some(tool_call_id),
                                        output: Some(image_path),
                                    });
                                }
                                Err(e) => {
                                    display_clone.print_debug(
                                        &format!("Failed to generate character image: {}", e),
                                        Color::Red,
                                    );
                                    let mut outputs = pending_tool_outputs_clone.lock().await;
                                    outputs.push(ToolsOutputs {
                                        tool_call_id: Some(tool_call_id),
                                        output: Some("Failed to generate image".to_string()),
                                    });
                                }
                            }
                        });

                        // Submit a dummy output immediately
                        let dummy_submit_request = SubmitToolOutputsRunRequest {
                            tool_outputs: vec![ToolsOutputs {
                                tool_call_id: Some(tool_call_id_clone.clone()),
                                output: Some("Tool started".to_string()),
                            }],
                            stream: None,
                        };
                        client
                            .threads()
                            .runs(&thread_id)
                            .submit_tool_outputs(&run.id, dummy_submit_request)
                            .await?;
                    }
                }
            }
        }

        // Wait for the run to complete after submitting tool outputs
        loop {
            let run_status = client.threads().runs(&thread_id).retrieve(&run.id).await?;
            display.print_debug(
                &format!("Debug: Current run status: {:?}", run_status.status),
                Color::Magenta,
            );

            match run_status.status {
                RunStatus::Completed => {
                    display.print_debug("Debug: Run completed", Color::Magenta);
                    break;
                }
                RunStatus::Failed => {
                    display.print_debug("Debug: Run failed", Color::Magenta);
                    return Err(SharadError::Other("Run failed".to_string()));
                }
                RunStatus::RequiresAction => {
                    display.print_debug("Debug: Run requires action", Color::Magenta);
                    break;
                }
                _ => {
                    display.print_thinking_dot();
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }

        display.print_debug("Debug: Getting latest message", Color::Magenta);
        let response_text = get_latest_message(client, &thread_id).await?;
        log_and_display_message(log_file, &response_text, "Assistant's response", display)?;
        generate_and_play_audio(audio, &response_text, "narrator").await?;
    }

    Ok(())
}

async fn create_and_wait_for_run(
    client: &Client<OpenAIConfig>,
    thread_id: &str,
    assistant_id: &str,
    display: &Display,
) -> Result<RunObject, SharadError> {
    display.print_debug("Debug: Creating run request", Color::Magenta);
    let run_request = CreateRunRequestArgs::default()
        .assistant_id(assistant_id)
        .parallel_tool_calls(false)
        .build()?;
    display.print_debug("Debug: Sending run request", Color::Magenta);
    let mut run = client.threads().runs(thread_id).create(run_request).await?;
    display.print_debug(
        &format!("Debug: Run created with ID: {}", run.id),
        Color::Magenta,
    );

    display.print_thinking();
    let mut iterations = 0;
    loop {
        iterations += 1;
        display.print_debug(
            &format!("Debug: Checking run status (iteration {})", iterations),
            Color::Magenta,
        );
        let run_status = client.threads().runs(thread_id).retrieve(&run.id).await?;
        display.print_debug(
            &format!("Debug: Current run status: {:?}", run_status.status),
            Color::Magenta,
        );

        match run_status.status {
            RunStatus::Completed => {
                display.print_debug("Debug: Run completed", Color::Magenta);
                run = run_status;
                break;
            }
            RunStatus::Failed => {
                display.print_debug("Debug: Run failed", Color::Magenta);
                return Err(SharadError::Other("Run failed".to_string()));
            }
            RunStatus::RequiresAction => {
                display.print_debug("Debug: Run requires action", Color::Magenta);
                run = run_status;
                break;
            }
            _ => {
                display.print_thinking_dot();
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
    display.clear_thinking();

    Ok(run)
}

async fn fetch_all_messages(
    client: &Client<OpenAIConfig>,
    thread_id: &str,
) -> Result<Vec<MessageObject>, SharadError> {
    let mut all_messages = Vec::new();
    let mut before: Option<String> = None;
    loop {
        let mut params = vec![("order", "desc"), ("limit", "100")];
        if let Some(before_id) = &before {
            params.push(("before", before_id));
        }
        let messages = client.threads().messages(thread_id).list(&params).await?;
        all_messages.extend(messages.data.into_iter().rev());
        if messages.has_more {
            before = messages.first_id;
        } else {
            break;
        }
    }
    Ok(all_messages)
}

fn display_message(message: &MessageObject, display: &Display) {
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

fn get_user_input(
    display: &Display,
) -> Pin<Box<dyn Future<Output = Result<String, SharadError>> + '_>> {
    Box::pin(async move {
        let user_input = record_and_transcribe_audio(display).await?;
        let corrected_input = correct_input(display, &user_input)?;
        if corrected_input.trim().is_empty() {
            display.print_wrapped("Input cannot be empty. Please try again.", Color::Red);
            return get_user_input(display).await;
        }
        Ok(corrected_input)
    })
}

async fn send_user_message(
    client: &Client<OpenAIConfig>,
    thread_id: &str,
    content: &str,
) -> Result<(), SharadError> {
    client
        .threads()
        .messages(thread_id)
        .create(
            CreateMessageRequestArgs::default()
                .role(MessageRole::User)
                .content(content)
                .build()?,
        )
        .await?;
    Ok(())
}

async fn get_latest_message(
    client: &Client<OpenAIConfig>,
    thread_id: &str,
) -> Result<String, SharadError> {
    let messages = client
        .threads()
        .messages(thread_id)
        .list(&[("limit", "1")])
        .await?;

    if let Some(latest_message) = messages.data.first() {
        if let Some(MessageContent::Text(text_content)) = latest_message.content.first() {
            return Ok(text_content.text.value.clone());
        }
    }
    Err(SharadError::Other("No message found".to_string()))
}

fn log_and_display_message(
    log_file: &mut File,
    message: &str,
    prefix: &str,
    display: &Display,
) -> Result<(), SharadError> {
    writeln!(log_file, "{}: {}", prefix, message)?;
    display.print_separator(Color::Cyan);
    display.print_wrapped(message, Color::Green);
    display.print_separator(Color::Cyan);
    Ok(())
}
