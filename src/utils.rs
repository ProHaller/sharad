use crate::display::Display;
use crate::Color;
use rand::{self, Rng};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use serde::Serialize;
use std::error::Error;
use std::fmt;

pub fn correct_input(display: &Display, initial_input: &str) -> Result<String, Box<dyn Error>> {
    let mut rl = DefaultEditor::new()?;

    // Read the line with the initial input
    match rl.readline_with_initial(">> ", (initial_input, "")) {
        Ok(line) => {
            // Clear the line by moving the cursor up and clearing it
            // Assuming the terminal supports ANSI escape codes
            print!("\x1B[1A\x1B[2K"); // Move cursor up and clear the line
            Ok(line)
        }
        Err(ReadlineError::Interrupted) => {
            display.print_wrapped("Input interrupted. Input 'exit' to exit.", Color::Red);
            Ok(String::new())
        }
        Err(ReadlineError::Eof) => {
            display.print_wrapped("End of input (Ctrl-D).", Color::Red);
            Ok(String::new())
        }
        Err(err) => {
            display.print_wrapped(&format!("Error: {:?}", err), Color::Red);
            Err(Box::new(err))
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
