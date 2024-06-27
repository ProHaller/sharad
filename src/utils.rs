use crate::display::Display;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute, terminal,
    terminal::ClearType,
};
use rand::{self, Rng};
use serde::Serialize;
use std::error::Error;
use std::fmt;
use std::io::{self, Write};
use std::time::Duration;
use tokio::process::Command;

pub fn correct_input(
    _display: &Display,
    initial_input: &str,
) -> Result<Option<String>, Box<dyn Error>> {
    terminal::enable_raw_mode()?;

    let mut input = String::from(initial_input);
    let mut cursor_position = input.len();

    loop {
        // Clear the current line and print the prompt and input
        execute!(
            io::stdout(),
            cursor::MoveToColumn(0),
            terminal::Clear(ClearType::CurrentLine),
        )?;
        print!(">> {}", input);

        // Move the cursor to the correct position
        execute!(
            io::stdout(),
            cursor::MoveToColumn((cursor_position + 3) as u16),
        )?;

        io::stdout().flush()?;

        // Wait for a key event with a short timeout
        if event::poll(Duration::from_millis(10))? {
            if let Event::Key(KeyEvent {
                code,
                kind: KeyEventKind::Press,
                modifiers,
                ..
            }) = event::read()?
            {
                match code {
                    KeyCode::Esc => {
                        terminal::disable_raw_mode()?;
                        println!(); // Move to the next line
                        return Ok(None);
                    }
                    KeyCode::Enter => {
                        terminal::disable_raw_mode()?;
                        println!(); // Move to the next line
                        return Ok(Some(input));
                    }
                    KeyCode::Char('v') if modifiers.contains(KeyModifiers::CONTROL) => {
                        // Handle paste (Ctrl+V)
                        if let Ok(clipboard) = cli_clipboard::get_contents() {
                            input.insert_str(cursor_position, &clipboard);
                            cursor_position += clipboard.len();
                        }
                    }
                    KeyCode::Char(c) => {
                        input.insert(cursor_position, c);
                        cursor_position += 1;
                    }
                    KeyCode::Backspace => {
                        if cursor_position > 0 {
                            input.remove(cursor_position - 1);
                            cursor_position -= 1;
                        }
                    }
                    KeyCode::Delete => {
                        if cursor_position < input.len() {
                            input.remove(cursor_position);
                        }
                    }
                    KeyCode::Left => {
                        cursor_position = cursor_position.saturating_sub(1);
                    }
                    KeyCode::Right => {
                        if cursor_position < input.len() {
                            cursor_position += 1;
                        }
                    }
                    KeyCode::Home => {
                        cursor_position = 0;
                    }
                    KeyCode::End => {
                        cursor_position = input.len();
                    }
                    _ => {}
                }
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RollResult {
    successes: u8,
    critical_successes: u8,
    glitch: bool,
    critical_glitch: bool,
    is_successful: bool,
}

impl fmt::Display for RollResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Roll Result:\n\
            - Successes: {}\n\
            - Critical Successes: {}\n\
            - Glitch: {}\n\
            - Critical Glitch: {}\n\
            - Task Successful: {}",
            self.successes,
            self.critical_successes,
            self.glitch,
            self.critical_glitch,
            self.is_successful
        )
    }
}

pub fn shadowrun_dice_roll(dice_number: u8, threshold: u8) -> RollResult {
    let mut rng = rand::thread_rng();
    let (mut successes, mut critical_successes, mut ones) = (0, 0, 0);

    for _ in 0..dice_number {
        let mut result = rng.gen_range(1..=6);
        loop {
            if result == 1 {
                ones += 1;
                break;
            } else if result == 6 {
                successes += 1;
                critical_successes += 1;
                result = rng.gen_range(1..=6);
            } else if result >= 5 {
                successes += 1;
                break;
            } else {
                break;
            }
        }
    }

    let glitch = ones as f32 / dice_number as f32 >= 0.5;
    let critical_glitch = glitch && successes == 0;
    let is_successful = successes >= threshold;

    RollResult {
        successes,
        critical_successes,
        glitch,
        critical_glitch,
        is_successful,
    }
}

pub fn open_image(path: &str) -> Result<(), std::io::Error> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd").args(["/C", "start", path]).spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(path).spawn()?;
    }

    Ok(())
}
