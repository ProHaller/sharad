use crate::display::Display;
use crate::error::SharadError;
use crate::image::{generate_character_image, Appearance, CharacterInfo};
use crate::settings::load_settings;
use crate::utils::{correct_input, shadowrun_dice_roll};

use crate::audio::{generate_and_play_audio, record_and_transcribe_audio};
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
use crossterm::{
    cursor, execute,
    terminal::{Clear, ClearType},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value;
use std::fs::{self, File};
use std::future::Future;
use std::io::{stdout, Write};
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

pub fn load_conversation_from_file(display: &Display, art: &str) -> Result<Save, SharadError> {
    fn draw_header(display: &Display, art: &str) -> Result<(), SharadError> {
        execute!(stdout(), Clear(ClearType::All), cursor::MoveTo(0, 0))?;
        display.print_centered(art, Color::Green);
        display.print_centered(
            &format!("Welcome to Sharad v{}", env!("CARGO_PKG_VERSION")),
            Color::Cyan,
        );
        display.print_centered("You can quit by inputing \"exit\".", Color::Yellow);
        display.print_separator(Color::Blue);
        Ok(())
    }

    draw_header(display, art)?;
    display.print_wrapped("Loading a game.", Color::Green);

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
            _ => {
                display.print_wrapped("Invalid choice. Please enter a valid number.", Color::Red);
                // Redraw the header and menu after an invalid choice
                draw_header(display, art)?;
                display.print_wrapped("Loading a game.", Color::Green);
                display.print_wrapped("Input '0' to go back to main menu.", Color::Yellow);
                display.print_wrapped("Available save files:", Color::Yellow);
                for (index, save_file) in save_files.iter().enumerate() {
                    display.print_wrapped(&format!("{}. {}", index + 1, save_file), Color::White);
                }
            }
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
    art: &str,
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
            .content(format!("You are the Game Master of a Role Playing Game. Start by welcoming the player to the game world and ask them to describe their character. The description should include the character's name, background, and motivations. Note that the player is considered a beginner in this world until they have gained significant experience. Write your response in valid JSON within a \"narration\" tag. Always write in the following language: {}", language))
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
        let save = load_conversation_from_file(display, art)?;
        (save.assistant_id, save.thread_id)
    };

    let response =
        run_conversation_with_save(log_file, &assistant_id, &thread_id, is_new_game, display)
            .await?;
    let json_response = json!({
        "assistant_id": assistant_id,
        "thread_id": thread_id,
        "response": response,
    });

    display.print_debug(
        &serde_json::to_string_pretty(&json_response)?,
        Color::Magenta,
    );
    Ok(())
}

pub async fn run_conversation_with_save(
    log_file: &mut File,
    assistant_id: &str,
    thread_id: &str,
    is_new_game: bool,
    display: &Display,
) -> Result<Value, SharadError> {
    let client = Client::new();
    let audio = Audio::new(&client);

    let _request = CreateRunRequestArgs::default()
    .assistant_id(assistant_id)
    .tools(vec![AssistantTools::Function(AssistantToolsFunction {
        function: FunctionObject {
            name: "generate_character_image".to_string(),
            description: Some("Generate a character image based on the provided details".to_string()),
            parameters: Some(json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The name of the character"
                    },
                    "appearance": {
                        "type": "object",
                        "description": "Details about the character's physical appearance in English",
                        "properties": {
                            "gender": {
                                "type": "string",
                                "description": "The character's gender in English"
                            },
                            "age": {
                                "type": "string",
                                "description": "The character's approximate age in English"
                            },
                            "height": {
                                "type": "string",
                                "description": "The character's height in English"
                            },
                            "build": {
                                "type": "string",
                                "description": "The character's body type in English"
                            },
                            "hair": {
                                "type": "string",
                                "description": "The character's hair color and style in English"
                            },
                            "eyes": {
                                "type": "string",
                                "description": "The character's eye color in English"
                            },
                            "skin": {
                                "type": "string",
                                "description": "The character's skin tone in English"
                            }
                        }
                    },
                    "distinctive_signs": {
                        "type": "array",
                        "description": "List of distinctive signs or features in English",
                        "items": {
                            "type": "string"
                        }
                    },
                    "accessories": {
                        "type": "array",
                        "description": "List of accessories worn by the character in English",
                        "items": {
                            "type": "string"
                        }
                    },
                    "location": {
                        "type": "string",
                        "description": "The specific location where the character is situated in English"
                    },
                    "ambiance": {
                        "type": "string",
                        "description": "The mood or atmosphere of the scene in English"
                    },
                    "environment": {
                        "type": "string",
                        "description": "The surrounding environment or setting in English"
                    },
                    "image_generation_prompt": {
                        "type": "string",
                        "description": "A detailed prompt for generating the character image on Dall-E following content Policy rules, in English"
                    }
                },
                "required": ["name", "appearance", "location", "environment", "image_generation_prompt"],
            })),
        },
    })])
    .build()?;

    if is_new_game {
        handle_new_game(&client, thread_id, assistant_id, log_file, display, &audio).await?;
    } else {
        display_previous_conversation(&client, thread_id, display).await?;
    }

    main_conversation_loop(&client, thread_id, assistant_id, log_file, display, &audio).await?;

    display.print_footer("Thank you for playing!");
    writeln!(log_file, "Conversation ended.")?;
    log_file.sync_all()?;

    // Serialize the final state to JSON
    let final_state = json!({
        "status": "Conversation ended",
        "assistant_id": assistant_id,
        "thread_id": thread_id,
    });

    Ok(final_state)
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
                "Game Master's initial message",
                display,
            )?;
            generate_and_play_audio(audio, response_text, "Game Master").await?;
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
    for message in &all_messages[1..] {
        display_message(message, display);
    }
    display.print_separator(Color::Cyan);
    display.print_wrapped("End of previous conversation.", Color::Yellow);

    Ok(())
}

async fn main_conversation_loop(
    client: &Client<OpenAIConfig>,
    thread_id: &str,
    assistant_id: &str,
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

        // Create the JSON structure
        let message_json = serde_json::json!({
            "instructions": "Act as a professional Game Master in a role-playing game. Evaluate the probability of success for each intended player action and roll the dice when pertinent. If an action falls outside the player's skills and capabilities, make them fail and face the consequences, which could include death. Allow the player to attempt one action at a time without providing choices. Do not allow the player to summon anything that was not previously introduced unless it is perfectly innocuous. For actions involving multiple steps or failure points, require the player to choose a course of action at each step. Write your reasoning and the results of the dice roll in a JSON \"reasoning\" tag and narrate the results in a JSON \"narration\" tag. Present one action at a time before prompting the player for their next action. Do not let the action stale, but keep things going.",
            "player_action": user_input
        });

        // Convert the JSON to a string
        let user_prompt = serde_json::to_string(&message_json)?;

        display.print_debug(
            &format!("Debug: Sending user message: {}", user_prompt),
            Color::Magenta,
        );
        display.print_wrapped(&user_input, Color::Blue);
        send_user_message(client, thread_id, &user_prompt).await?;

        // Ensure there is no active run before creating a new one
        loop {
            let runs = client
                .threads()
                .runs(thread_id)
                .list(&[("limit", "1")])
                .await?;
            if runs.data.is_empty() || runs.data[0].status == RunStatus::Completed {
                break;
            }
            display.print_debug("Debug: Waiting for active run to complete", Color::Magenta);
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        display.print_debug("Debug: Creating and waiting for run", Color::Magenta);
        let run = create_and_wait_for_run(client, thread_id, assistant_id, display).await?;

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
                    if tool_call.function.name == "roll_dice" {
                        let args: serde_json::Value =
                            serde_json::from_str(&tool_call.function.arguments)?;
                        let dice_number = args["dice_number"].as_u64().unwrap_or(0) as u8;
                        let threshold = args["threshold"].as_u64().unwrap_or(0) as u8;

                        let roll_result = shadowrun_dice_roll(dice_number, threshold);
                        let tool_output = serde_json::to_string(&roll_result)?;

                        let tool_call_id = tool_call.id.clone();
                        let pending_tool_outputs_clone = Arc::clone(&pending_tool_outputs);

                        let mut outputs = pending_tool_outputs_clone.lock().await;
                        let tool_output_clone = tool_output.clone();
                        let tool_call_id_clone = tool_call_id.clone();
                        outputs.push(ToolsOutputs {
                            tool_call_id: Some(tool_call_id),
                            output: Some(tool_output),
                        });

                        // Submit the tool output immediately
                        let submit_request = SubmitToolOutputsRunRequest {
                            tool_outputs: vec![ToolsOutputs {
                                tool_call_id: Some(tool_call_id_clone),
                                output: Some(tool_output_clone),
                            }],
                            stream: None,
                        };
                        client
                            .threads()
                            .runs(thread_id)
                            .submit_tool_outputs(&run.id, submit_request)
                            .await?;
                    }
                    if tool_call.function.name == "generate_character_image" {
                        let args: serde_json::Value =
                            serde_json::from_str(&tool_call.function.arguments)?;
                        let character_info = CharacterInfo {
                            name: args["name"].as_str().unwrap_or("").to_string(),
                            appearance: Appearance {
                                gender: args["appearance"]["gender"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string(),
                                age: args["appearance"]["age"].as_str().unwrap_or("").to_string(),
                                height: args["appearance"]["height"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string(),
                                build: args["appearance"]["build"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string(),
                                hair: args["appearance"]["hair"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string(),
                                eyes: args["appearance"]["eyes"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string(),
                                skin: args["appearance"]["skin"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string(),
                            },
                            distinctive_signs: args["distinctive_signs"]
                                .as_array()
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default(),
                            accessories: args["accessories"]
                                .as_array()
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default(),
                            location: args["location"].as_str().unwrap_or("").to_string(),
                            ambiance: args["ambiance"].as_str().unwrap_or("").to_string(),
                            environment: args["environment"].as_str().unwrap_or("").to_string(),
                            image_generation_prompt: args["image_generation_prompt"]
                                .as_str()
                                .unwrap_or("")
                                .to_string(),
                        };
                        let tool_call_id = tool_call.id.clone();
                        let tool_call_id_clone = tool_call.id.clone();
                        let pending_tool_outputs_clone = Arc::clone(&pending_tool_outputs);
                        let display_clone = display.clone();

                        // Spawn a new task to handle image generation
                        spawn(async move {
                            match generate_character_image(character_info).await {
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
                            .runs(thread_id)
                            .submit_tool_outputs(&run.id, dummy_submit_request)
                            .await?;
                    }
                }
            }
        }

        // Wait for the run to complete after submitting tool outputs
        loop {
            let run_status = client.threads().runs(thread_id).retrieve(&run.id).await?;
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
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }

        display.print_debug("Debug: Getting latest message", Color::Magenta);
        let response_text = get_latest_message(client, thread_id).await?;
        log_and_display_message(log_file, &response_text, "Game Master", display)?;

        // Parse the JSON response to extract the narration for audio
        let json_response: Value = serde_json::from_str(&response_text)?;
        if let Some(narration) = json_response.get("Narration") {
            generate_and_play_audio(audio, narration.as_str().unwrap_or(""), "narrator").await?;
        }
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
    let max_iterations = 100; // Set a reasonable maximum number of iterations

    loop {
        iterations += 1;
        if iterations > max_iterations {
            display.clear_thinking();
            return Err(SharadError::Other(
                "Run exceeded maximum iterations".to_string(),
            ));
        }

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
        MessageRole::Assistant => "Game Master",
    };
    display.print_separator(Color::Cyan);
    display.print_wrapped(&format!("{}: ", role), Color::Yellow);

    if let Some(MessageContent::Text(text_content)) = message.content.first() {
        let text = &text_content.text.value;
        match message.role {
            MessageRole::User => {
                if let Ok(json) = serde_json::from_str::<Value>(text) {
                    if let Some(instructions) = json.get("instructions") {
                        display.print_debug(
                            &format!("instructions: {}", instructions),
                            Color::Magenta,
                        );
                    }
                    if let Some(player_action) = json.get("player_action") {
                        display.print_debug(&format!("{}", player_action), Color::Blue);
                    }
                }
            }
            MessageRole::Assistant => {
                // Try to parse the text as JSON
                if let Ok(json) = serde_json::from_str::<Value>(text) {
                    if let Some(reasoning) = json.get("reasoning") {
                        display.print_debug(&format!("Reasoning: {}", reasoning), Color::Magenta);
                    }
                    // Display instructions and Game Master Reasoning as debug
                    if let Some(narration) = json.get("narration") {
                        display.print_debug(&format!("{}", narration), Color::Green);
                    }

                    // Display Narration in green
                    if let Some(narration) = json.get("Narration") {
                        display.print_wrapped(narration.as_str().unwrap_or(""), Color::Green);
                    }
                } else {
                    // If it's not valid JSON, just display the text as before
                    display.print_wrapped(text, Color::Green);
                }
            }
        }
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
    sender: &str,
    display: &Display,
) -> Result<(), SharadError> {
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let log_entry = format!("[{}] {}: {}\n", timestamp, sender, message);
    log_file.write_all(log_entry.as_bytes())?;
    // Parse the JSON message
    let json: Value = serde_json::from_str(message)?;

    match sender {
        "You" => {
            if let Some(instructions) = json.get("instructions") {
                display.print_debug(&format!("Instructions: {}", instructions), Color::Magenta);
            }
            if let Some(player_action) = json.get("player_action") {
                display.print_debug(&format!("{}", player_action), Color::Blue);
            }
        }

        "Game Master" => {
            // Display instructions and Game Master Reasoning as debug
            if let Some(reasoning) = json.get("reasoning") {
                display.print_debug(&format!("reasoning: {}", reasoning), Color::Magenta);
            }
            if let Some(narration) = json.get("narration") {
                display.print_wrapped(&format!("{}", narration), Color::Green);
            }

            // Display Narration in green
            if let Some(narration) = json.get("Narration") {
                display.print_wrapped(narration.as_str().unwrap_or(""), Color::Green);
            }
        }
        _ => {
            if let Some(narration) = json.get("narration") {
                display.print_wrapped(narration.as_str().unwrap_or(""), Color::Green);
            }
        }
    }

    Ok(())
}
