use crate::assistant::{
    load_conversation_from_file, run_conversation, run_conversation_with_save, Save, SAVE_DIR,
};
use crate::display::Display;
use crate::error::SharadError;
use crate::image;
use crate::settings::{load_settings, save_settings, validate_settings, Settings};

use crossterm::style::Color;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind},
    execute, style,
    style::SetForegroundColor,
    terminal::{self, Clear, ClearType},
};
use std::fs::File;
use std::io;
use std::io::{stdout, Write};
use std::path::Path;
use tokio::fs;

pub const MAIN_MENU_ITEMS: [&str; 5] = [
    "Start a new game",
    "Load a game",
    "Create an image",
    "Settings",
    "Exit",
];

const ART_HEIGHT: u16 = 37;
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
    let mut display = Display::new();
    display_art(&mut display)?;
    let mut settings = load_settings()?;
    validate_settings(&mut settings, &mut display).await?;

    terminal::enable_raw_mode()?;

    let mut selected = 0;
    let menu_items_count = MAIN_MENU_ITEMS.len();

    loop {
        draw_menu(&display, selected)?;

        if let Ok(Event::Key(key_event)) = event::read() {
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
                            &mut display,
                            &mut settings,
                        )
                        .await?
                        {
                            break;
                        }
                    }
                    KeyCode::Esc => {
                        break;
                    }
                    KeyCode::Char(c) => {
                        if let Some(index) = c
                            .to_digit(10)
                            .and_then(|d| d.checked_sub(1))
                            .map(|d| d as usize)
                        {
                            if index < menu_items_count
                                && handle_main_menu_selection(
                                    &mut log_file,
                                    index,
                                    &mut display,
                                    &mut settings,
                                )
                                .await?
                            {
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    terminal::disable_raw_mode()?;
    println!();
    display.print_footer("Thank you for playing Sharad!");
    println!();
    Ok(())
}

pub async fn change_settings(
    settings: &mut Settings,
    display: &mut Display,
) -> Result<(), SharadError> {
    display_art(display)?;

    terminal::enable_raw_mode()?;

    let mut selected = 0;
    let menu_items_count = SETTINGS_MENU_ITEMS.len();

    loop {
        draw_settings_menu(display, settings, selected)?;

        if let Ok(Event::Key(key_event)) = event::read() {
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
                    KeyCode::Esc => {
                        save_settings(settings)?;
                        break;
                    }
                    KeyCode::Char(c) => {
                        if let Some(digit) = c.to_digit(10) {
                            if digit > 0 && digit <= menu_items_count as u32 {
                                let index = (digit - 1) as usize;
                                if handle_settings_selection(settings, display, index).await? {
                                    break;
                                }
                            }
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

fn draw_menu(display: &Display, selected: usize) -> Result<(), SharadError> {
    clear_menu_area()?;

    let mut current_line = ART_HEIGHT + 1;

    print_centered_line(display, "Main Menu", Color::Green, current_line)?;
    current_line += 2;

    let max_width = MAIN_MENU_ITEMS
        .iter()
        .map(|item| item.len())
        .max()
        .unwrap_or(0);

    let (term_width, _) = terminal::size()?;
    let left_margin = (term_width - max_width as u16) / 2;

    for (i, item) in MAIN_MENU_ITEMS.iter().enumerate() {
        let prefix = if i == selected { "> " } else { "  " };
        let color = if i == selected {
            Color::Green
        } else {
            Color::White
        };
        let numbered_item = format!("{}{}. {}", prefix, i + 1, item);

        execute!(
            io::stdout(),
            cursor::MoveTo(left_margin, current_line),
            SetForegroundColor(color)
        )?;
        print!("{}", numbered_item);
        execute!(io::stdout(), style::ResetColor)?;

        current_line += 1;
    }

    io::stdout().flush()?;
    Ok(())
}

fn draw_settings_menu(
    display: &Display,
    settings: &Settings,
    selected: usize,
) -> Result<(), SharadError> {
    clear_menu_area()?;

    let mut current_line = ART_HEIGHT + 1;

    print_centered_line(display, "Settings Menu", Color::Green, current_line)?;
    current_line += 2;

    let max_width = SETTINGS_MENU_ITEMS
        .iter()
        .enumerate()
        .map(|(i, item)| match i {
            0 => item.len() + settings.language.len() + 13,
            2..=4 => item.len() + 7,
            _ => item.len(),
        })
        .max()
        .unwrap_or(0);

    let (term_width, _) = terminal::size()?;
    let left_margin = (term_width - max_width as u16) / 2;

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

        execute!(
            io::stdout(),
            cursor::MoveTo(left_margin, current_line),
            SetForegroundColor(color)
        )?;
        print!("{}", display_text);
        execute!(io::stdout(), style::ResetColor)?;

        current_line += 1;
    }

    io::stdout().flush()?;
    Ok(())
}

fn clear_menu_area() -> Result<(), SharadError> {
    execute!(
        io::stdout(),
        cursor::MoveTo(0, ART_HEIGHT),
        Clear(ClearType::FromCursorDown)
    )?;
    io::stdout().flush()?;
    Ok(())
}

fn print_centered_line(
    _display: &Display,
    text: &str,
    color: Color,
    line: u16,
) -> Result<(), SharadError> {
    let (term_width, _) = terminal::size()?;
    let start_x = (term_width - text.len() as u16) / 2;
    execute!(
        io::stdout(),
        cursor::MoveTo(start_x, line),
        SetForegroundColor(color)
    )?;
    print!("{}", text);
    execute!(io::stdout(), style::ResetColor)?;
    io::stdout().flush()?;
    Ok(())
}

async fn handle_main_menu_selection(
    log_file: &mut File,
    selected: usize,
    display: &mut Display,
    settings: &mut Settings,
) -> Result<bool, SharadError> {
    terminal::disable_raw_mode()?;

    let should_exit = match selected {
        0 => {
            match run_conversation(log_file, true, display).await {
                Ok(_) => {
                    display.print_wrapped("Conversation completed successfully.", Color::Green)
                }
                Err(e) => {
                    display.print_wrapped(&format!("Failed to run conversation: {}", e), Color::Red)
                }
            }
            false
        }
        1 => {
            display.print_wrapped("Loading a game.", Color::Green);
            match load_conversation_from_file(display).await {
                Ok(save) => {
                    match run_conversation_with_save(
                        log_file,
                        &save.assistant_id,
                        &save.thread_id,
                        false,
                        display,
                    )
                    .await
                    {
                        Ok(_) => display.print_wrapped(
                            "Saved game loaded and conversation completed successfully.",
                            Color::Green,
                        ),
                        Err(e) => display.print_wrapped(
                            &format!("Failed to run conversation with save: {}", e),
                            Color::Red,
                        ),
                    }
                }
                Err(e) => display.print_wrapped(&format!("Failed to load game: {}", e), Color::Red),
            }
            false
        }
        2 => {
            match display.get_user_input("What image would you like to generate?")? {
                Some(prompt) => match image::generate_and_save_image(&prompt).await {
                    Ok(_) => display
                        .print_wrapped("Image generated and saved successfully.", Color::Green),
                    Err(e) => display
                        .print_wrapped(&format!("Failed to generate image: {}", e), Color::Red),
                },
                None => display.print_wrapped("Image generation cancelled.", Color::Yellow),
            }
            false
        }
        3 => {
            match change_settings(settings, display).await {
                Ok(_) => display.print_wrapped("Settings updated successfully.", Color::Green),
                Err(e) => {
                    display.print_wrapped(&format!("Failed to change settings: {}", e), Color::Red)
                }
            }
            false
        }
        4 => {
            display.print_wrapped("Exiting game.", Color::Green);
            true
        }
        _ => {
            return Err(SharadError::InvalidMenuSelection(String::from(
                "Invalid menu selection.",
            )))
        }
    };

    display_art(display)?;
    terminal::enable_raw_mode()?;
    Ok(should_exit)
}

async fn handle_settings_selection(
    settings: &mut Settings,
    display: &mut Display,
    selected: usize,
) -> Result<bool, SharadError> {
    terminal::disable_raw_mode()?;

    let should_exit = match selected {
        0 => {
            match display.get_user_input("Enter the language you want to play in:")? {
                Some(new_language) if !new_language.trim().is_empty() => {
                    settings.language = new_language.trim().to_string();
                    display.print_wrapped(
                        &format!("Language changed to {}.", settings.language),
                        Color::Green,
                    );
                }
                Some(_) => display
                    .print_wrapped("Language cannot be empty. No changes made.", Color::Yellow),
                None => display.print_wrapped("Language change cancelled.", Color::Yellow),
            }
            false
        }
        1 => {
            settings.openai_api_key.clear();
            validate_settings(settings, display).await?;
            display.print_wrapped("OpenAI API key cleared and re-validated.", Color::Green);
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
        5 => match save_settings(settings) {
            Ok(_) => {
                display.print_wrapped("Settings saved successfully.", Color::Green);
                true
            }
            Err(e) => {
                display.print_wrapped(&format!("Failed to save settings: {}", e), Color::Red);
                false
            }
        },
        _ => {
            return Err(SharadError::InvalidMenuSelection(String::from(
                "Invalid menu selection.",
            )))
        }
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
                    KeyCode::Esc => {
                        terminal::disable_raw_mode()?;
                        return Ok(None);
                    }
                    KeyCode::Char(c) => {
                        if let Some(digit) = c.to_digit(10) {
                            if digit > 0 && digit <= menu_items_count as u32 {
                                let index = (digit - 1) as usize;
                                terminal::disable_raw_mode()?;
                                return if index == menu_items_count - 1 {
                                    Ok(None)
                                } else {
                                    Ok(Some(assistants[index].0.clone()))
                                };
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

pub async fn load_game_menu(display: &mut Display) -> Result<Option<Save>, SharadError> {
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
        display.get_user_input("Press Enter to continue...")?;
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
                    KeyCode::Esc => {
                        terminal::disable_raw_mode()?;
                        return Ok(None);
                    }
                    KeyCode::Char(c) => {
                        if let Some(index) = c
                            .to_digit(10)
                            .and_then(|d| (d as usize).checked_sub(1))
                            .filter(|&i| i < menu_items_count)
                        {
                            terminal::disable_raw_mode()?;
                            return handle_load_game_selection(save_dir, &menu_items, index).await;
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

pub fn display_art(display: &mut Display) -> Result<(), SharadError> {
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
