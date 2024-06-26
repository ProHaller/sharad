use crate::assistant::{
    load_conversation_from_file, run_conversation, run_conversation_with_save, Save, SAVE_DIR,
};
use crate::display::Display;
use crate::error::SharadError;
use crate::image;
use crate::settings::{load_settings, save_settings, validate_settings, Settings};

use colored::*;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{self, Clear, ClearType},
};
use std::collections::VecDeque;
use std::fs::File;
use std::io::stdout;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::time::sleep;

pub const MAIN_MENU_ITEMS: [&str; 5] = [
    "Start a new game",
    "Load a game",
    "Create an image",
    "Settings",
    "Exit",
];

const ART_HEIGHT: u16 = 40;
pub const ART: &str = r#"







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
     ----------------------------------------------------------------------------- 

  _____ _                         _
 / ____| |                       | |
| (___ | |__   __ _ _ __ __ _  __| |
 \___ \| '_ \ / _` | '__/ _` |/ _` |
 ____) | | | | (_| | | | (_| | (_| |
|_____/|_| |_|\__,_|_|  \__,_|\__,_|
    "#;

pub async fn main_menu(mut log_file: File) -> Result<(), SharadError> {
    let display = Display::new();
    display_art(&display)?;
    let mut settings = load_settings()?;
    validate_settings(&mut settings, &display).await?;

    terminal::enable_raw_mode()?;

    let mut selected = 0;
    let menu_items_count = MAIN_MENU_ITEMS.len();

    loop {
        draw_menu(&display, selected)?;

        if let Ok(key_event) = get_input_with_delay().await {
            if key_event.kind == KeyEventKind::Press {
                match key_event.code {
                    KeyCode::Up => {
                        selected = (selected + menu_items_count - 1) % menu_items_count;
                    }
                    KeyCode::Down => {
                        selected = (selected + 1) % menu_items_count;
                    }
                    KeyCode::Enter => {
                        if handle_main_menu_selection(
                            &mut log_file,
                            selected,
                            &display,
                            &mut settings,
                        )
                        .await?
                        {
                            break;
                        }
                    }
                    KeyCode::Char(c) => {
                        if ('1'..='5').contains(&c) {
                            let index = c as usize - '1' as usize;
                            if index < menu_items_count
                                && handle_main_menu_selection(
                                    &mut log_file,
                                    index,
                                    &display,
                                    &mut settings,
                                )
                                .await?
                            {
                                break;
                            }
                        } else if c == 'q' {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    terminal::disable_raw_mode()?;
    display.print_footer("Thank you for playing Sharad!");
    Ok(())
}

async fn handle_main_menu_selection(
    log_file: &mut File,
    selected: usize,
    display: &Display,
    settings: &mut Settings,
) -> Result<bool, SharadError> {
    terminal::disable_raw_mode()?;
    let should_exit = match selected {
        0 => {
            if let Err(e) = run_conversation(log_file, true, display).await {
                display.print_wrapped(&format!("Failed to run conversation: {}", e), Color::Red);
            }
            false
        }
        1 => {
            display.print_wrapped("Loading a game.", Color::Green);
            match load_conversation_from_file(display).await {
                Ok(save) => {
                    if let Err(e) = run_conversation_with_save(
                        log_file,
                        &save.assistant_id,
                        &save.thread_id,
                        false,
                        display,
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
            false
        }
        2 => {
            let prompt = display.get_user_input("What image would you like to generate?");
            if let Err(e) = image::generate_and_save_image(&prompt).await {
                display.print_wrapped(&format!("Failed to generate image: {}", e), Color::Red);
            }
            false
        }
        3 => {
            if let Err(e) = change_settings(settings, display).await {
                display.print_wrapped(&format!("Failed to change settings: {}", e), Color::Red);
            }
            false
        }
        4 => {
            display.print_wrapped("Exiting game.", Color::Green);
            true
        }
        _ => unreachable!(),
    };
    display_art(display)?; // Redisplay the art after user input
    terminal::enable_raw_mode()?;
    Ok(should_exit)
}

pub async fn change_settings(
    settings: &mut Settings,
    display: &Display,
) -> Result<(), SharadError> {
    display_art(display)?;

    terminal::enable_raw_mode()?;

    let mut selected = 0;
    let menu_items_count = SETTINGS_MENU_ITEMS.len();

    loop {
        draw_settings_menu(display, settings, selected)?;

        if let Event::Key(key_event) = event::read()? {
            if key_event.kind == KeyEventKind::Press {
                match key_event.code {
                    KeyCode::Up => {
                        selected = (selected + menu_items_count - 1) % menu_items_count;
                    }
                    KeyCode::Down => {
                        selected = (selected + 1) % menu_items_count;
                    }
                    KeyCode::Enter => {
                        if handle_settings_selection(settings, display, selected).await? {
                            break;
                        }
                    }
                    KeyCode::Char(c) => {
                        if ('1'..='6').contains(&c) {
                            let index = c as usize - '1' as usize;
                            if index < menu_items_count
                                && handle_settings_selection(settings, display, index).await?
                            {
                                break;
                            }
                        } else if c == 'q' {
                            save_settings(settings)?;
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    terminal::disable_raw_mode()?;
    Ok(())
}

async fn handle_settings_selection(
    settings: &mut Settings,
    display: &Display,
    selected: usize,
) -> Result<bool, SharadError> {
    terminal::disable_raw_mode()?;
    let should_exit = match selected {
        0 => {
            let new_language = display.get_user_input("Enter the language you want to play in:");
            if !new_language.trim().is_empty() {
                settings.language = new_language;
                display.print_wrapped(
                    &format!("Language changed to {}.", settings.language),
                    Color::Green,
                );
            } else {
                display.print_wrapped("Language cannot be empty. Please try again.", Color::Red);
            }
            false
        }
        1 => {
            settings.openai_api_key.clear();
            validate_settings(settings, display).await?;
            false
        }
        2 => {
            settings.audio_output_enabled = !settings.audio_output_enabled;
            display.print_wrapped(
                &format!("Audio Output Enabled: {}", settings.audio_output_enabled),
                Color::Green,
            );
            false
        }
        3 => {
            settings.audio_input_enabled = !settings.audio_input_enabled;
            display.print_wrapped(
                &format!("Audio Input Enabled: {}", settings.audio_input_enabled),
                Color::Green,
            );
            false
        }
        4 => {
            settings.debug_mode = !settings.debug_mode;
            display.print_wrapped(
                &format!("Debug Mode: {}", settings.debug_mode),
                Color::Green,
            );
            false
        }
        5 => {
            save_settings(settings)?;
            true
        }
        _ => unreachable!(),
    };
    terminal::enable_raw_mode()?;
    Ok(should_exit)
}

pub async fn choose_assistant(
    assistants: Vec<(String, String)>,
    display: &Display,
) -> Result<Option<String>, SharadError> {
    let mut menu_items = assistants
        .iter()
        .map(|(_, name)| name.clone())
        .collect::<Vec<String>>();
    menu_items.push("Return to Main Menu".to_string());

    let mut selected = 0;
    let menu_items_count = menu_items.len();

    terminal::enable_raw_mode()?;

    loop {
        draw_assistant_menu(display, &menu_items, selected)?;

        if let Event::Key(key_event) = event::read()? {
            if key_event.kind == KeyEventKind::Press {
                match key_event.code {
                    KeyCode::Up => {
                        selected = (selected + menu_items_count - 1) % menu_items_count;
                    }
                    KeyCode::Down => {
                        selected = (selected + 1) % menu_items_count;
                    }
                    KeyCode::Enter => {
                        terminal::disable_raw_mode()?;
                        return if selected == menu_items_count - 1 {
                            Ok(None)
                        } else {
                            Ok(Some(assistants[selected].0.clone()))
                        };
                    }
                    KeyCode::Char(c) => {
                        if c >= '1' && c <= (menu_items_count as u8 + b'0') as char {
                            let index = c as usize - '1' as usize;
                            terminal::disable_raw_mode()?;
                            return if index == menu_items_count - 1 {
                                Ok(None)
                            } else {
                                Ok(Some(assistants[index].0.clone()))
                            };
                        } else if c == 'q' {
                            terminal::disable_raw_mode()?;
                            return Ok(None);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

pub async fn load_game_menu(display: &Display) -> Result<Option<Save>, SharadError> {
    let save_dir = Path::new(SAVE_DIR);

    // Check if the save directory exists
    if !save_dir.exists() {
        display.print_wrapped("No save folder found. Creating one now.", Color::Yellow);
        fs::create_dir_all(save_dir)
            .await
            .map_err(SharadError::Io)?;
        return Ok(None);
    }

    let mut save_files = Vec::new();
    let mut dir = fs::read_dir(save_dir).await.map_err(SharadError::Io)?;

    while let Some(entry) = dir.next_entry().await.map_err(SharadError::Io)? {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|os_str| os_str.to_str()) == Some("json") {
            if let Some(file_stem) = path.file_stem().and_then(|os_str| os_str.to_str()) {
                save_files.push(file_stem.to_string());
            }
        }
    }

    if save_files.is_empty() {
        display.print_wrapped("No save files found.", Color::Yellow);
        display.get_user_input("Press Enter to continue...");
        return Ok(None);
    }

    let mut menu_items = save_files;
    menu_items.push("Return to Main Menu".to_string());

    let mut selected = 0;
    let menu_items_count = menu_items.len();

    terminal::enable_raw_mode()?;

    loop {
        draw_load_game_menu(display, &menu_items, selected)?;

        if let Event::Key(key_event) = event::read()? {
            if key_event.kind == KeyEventKind::Press {
                match key_event.code {
                    KeyCode::Up => {
                        selected = (selected + menu_items_count - 1) % menu_items_count;
                    }
                    KeyCode::Down => {
                        selected = (selected + 1) % menu_items_count;
                    }
                    KeyCode::Enter => {
                        terminal::disable_raw_mode()?;
                        return handle_load_game_selection(save_dir, &menu_items, selected).await;
                    }
                    KeyCode::Char(c) => {
                        if c >= '1' && c <= (menu_items_count as u8 + b'0') as char {
                            let index = c as usize - '1' as usize;
                            terminal::disable_raw_mode()?;
                            return handle_load_game_selection(save_dir, &menu_items, index).await;
                        } else if c == 'q' {
                            terminal::disable_raw_mode()?;
                            return Ok(None);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn handle_load_game_selection(
    save_dir: &Path,
    menu_items: &[String],
    selected: usize,
) -> Result<Option<Save>, SharadError> {
    if selected == menu_items.len() - 1 {
        Ok(None)
    } else {
        let save_file = save_dir.join(format!("{}.json", menu_items[selected]));
        let data = fs::read_to_string(save_file)
            .await
            .map_err(SharadError::Io)?;
        let save: Save = serde_json::from_str(&data).map_err(SharadError::SerdeJson)?;
        Ok(Some(save))
    }
}

pub fn draw_menu(display: &Display, selected: usize) -> Result<(), SharadError> {
    clear_menu_area()?;

    let mut current_line = ART_HEIGHT + 1; // Start one line below the art

    // Print the "Main Menu" title
    print_centered_line(display, "Main Menu", Color::Green, current_line)?;
    current_line += 1;

    // Add an empty line after the title
    current_line += 1;

    for (i, item) in MAIN_MENU_ITEMS.iter().enumerate() {
        let prefix = if i == selected { "> " } else { "  " };
        let color = if i == selected {
            Color::Green
        } else {
            Color::White
        };
        let numbered_item = format!("{}{}. {}", prefix, i + 1, item);
        print_centered_line(display, &numbered_item, color, current_line)?;
        current_line += 1;
    }

    Ok(())
}

pub fn draw_settings_menu(
    display: &Display,
    settings: &Settings,
    selected: usize,
) -> Result<(), SharadError> {
    clear_menu_area()?;

    let mut current_line = ART_HEIGHT + 1; // Start one line below the art

    // Print the "Settings Menu" title
    print_centered_line(display, "Settings Menu", Color::Green, current_line)?;
    current_line += 1;

    // Add an empty line after the title
    current_line += 1;

    for (i, item) in SETTINGS_MENU_ITEMS.iter().enumerate() {
        let prefix = if i == selected { "> " } else { "  " };
        let color = if i == selected {
            Color::Green
        } else {
            Color::White
        };

        let display_text = match i {
            0 => format!(
                "{}{}. {} (Current: {})",
                prefix,
                i + 1,
                item,
                settings.language
            ),
            1 => format!("{}{}. {}", prefix, i + 1, item),
            2 => format!(
                "{}{}. {} ({})",
                prefix,
                i + 1,
                item,
                settings.audio_output_enabled
            ),
            3 => format!(
                "{}{}. {} ({})",
                prefix,
                i + 1,
                item,
                settings.audio_input_enabled
            ),
            4 => format!("{}{}. {} ({})", prefix, i + 1, item, settings.debug_mode),
            5 => format!("{}{}. {}", prefix, i + 1, item),
            _ => unreachable!(),
        };

        print_centered_line(display, &display_text, color, current_line)?;
        current_line += 1;
    }

    Ok(())
}

fn draw_assistant_menu(
    display: &Display,
    menu_items: &[String],
    selected: usize,
) -> Result<(), SharadError> {
    clear_menu_area()?;

    let mut current_line = ART_HEIGHT + 1; // Start one line below the art

    // Print the "Choose Game Cartridge" title
    print_centered_line(display, "Choose Game Cartridge", Color::Green, current_line)?;
    current_line += 1;

    // Add an empty line after the title
    current_line += 1;

    for (i, item) in menu_items.iter().enumerate() {
        let prefix = if i == selected { "> " } else { "  " };
        let color = if i == selected {
            Color::Green
        } else {
            Color::White
        };
        let numbered_item = format!("{}{}. {}", prefix, i + 1, item);
        print_centered_line(display, &numbered_item, color, current_line)?;
        current_line += 1;
    }

    Ok(())
}

fn draw_load_game_menu(
    display: &Display,
    menu_items: &[String],
    selected: usize,
) -> Result<(), SharadError> {
    clear_menu_area()?;

    let mut current_line = ART_HEIGHT + 1; // Start one line below the art

    // Print the "Load Game" title
    print_centered_line(display, "Load Game", Color::Green, current_line)?;
    current_line += 1;

    // Add an empty line after the title
    current_line += 1;

    for (i, item) in menu_items.iter().enumerate() {
        let prefix = if i == selected { "> " } else { "  " };
        let color = if i == selected {
            Color::Green
        } else {
            Color::White
        };
        let numbered_item = format!("{}{}. {}", prefix, i + 1, item);
        print_centered_line(display, &numbered_item, color, current_line)?;
        current_line += 1;
    }

    Ok(())
}

fn clear_menu_area() -> Result<(), SharadError> {
    execute!(
        stdout(),
        cursor::MoveTo(0, ART_HEIGHT),
        Clear(ClearType::FromCursorDown)
    )?;
    Ok(())
}

fn print_centered_line(
    display: &Display,
    text: &str,
    color: Color,
    line: u16,
) -> Result<(), SharadError> {
    execute!(stdout(), cursor::MoveTo(0, line))?;
    display.print_centered(text, color);
    Ok(())
}

pub fn display_art(display: &Display) -> Result<(), SharadError> {
    display.print_centered(ART, Color::Green);
    display.print_centered(
        &format!("Welcome to Sharad v{}", env!("CARGO_PKG_VERSION")),
        Color::Cyan,
    );
    execute!(stdout(), Clear(ClearType::All), cursor::MoveTo(0, 0))?;
    display.print_centered(ART, Color::Green);
    display.print_centered(
        &format!("Welcome to Sharad v{}", env!("CARGO_PKG_VERSION")),
        Color::Cyan,
    );
    display.print_centered(
        "Use arrow keys to navigate, Enter to select, 'q' to quit.",
        Color::Yellow,
    );
    display.print_separator(Color::Blue);
    Ok(())
}

pub const SETTINGS_MENU_ITEMS: [&str; 6] = [
    "Change Language",
    "Change OpenAI API Key",
    "Toggle Audio Output",
    "Toggle Audio Input",
    "Toggle Debug Mode",
    "Back to Main Menu",
];

async fn get_input_with_delay() -> Result<KeyEvent, SharadError> {
    let mut last_key_time = Instant::now();
    let delay = Duration::from_millis(50);
    let mut recent_events = VecDeque::new();

    loop {
        if let Event::Key(key) = event::read()? {
            let now = Instant::now();
            if now.duration_since(last_key_time) > delay {
                // Check if this event is a duplicate of the previous one
                if let Some(last_event) = recent_events.back() {
                    if *last_event == key {
                        // If it's a duplicate, ignore it
                        continue;
                    }
                }

                // Add this event to our recent events
                recent_events.push_back(key);
                if recent_events.len() > 3 {
                    recent_events.pop_front();
                }

                last_key_time = now;
                return Ok(key);
            }
        }
        sleep(Duration::from_millis(10)).await;
    }
}
