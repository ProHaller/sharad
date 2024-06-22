use crate::display::Display;
use crate::Color;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::error::Error;

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
