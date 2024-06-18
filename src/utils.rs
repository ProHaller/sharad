use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::error::Error;
use std::io::{self, Write};

pub fn get_user_input(prompt: &str) -> String {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

pub fn correct_input(initial_input: &str) -> Result<String, Box<dyn Error>> {
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
            println!("CTRL-C");
            Ok(String::new())
        }
        Err(ReadlineError::Eof) => {
            println!("CTRL-D");
            Ok(String::new())
        }
        Err(err) => {
            println!("Error: {:?}", err);
            Err(Box::new(err))
        }
    }
}
