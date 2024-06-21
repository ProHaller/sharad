use colored::*;
use std::io::{self, Write};
use textwrap::{fill, wrap};
use unicode_width::UnicodeWidthStr;

pub struct Display {
    term_width: usize,
}

impl Display {
    pub fn new() -> Self {
        let term_width = term_size::dimensions().map_or(80, |(w, _)| w);
        Display { term_width }
    }

    pub fn print_centered(&self, text: &str, color: Color) {
        let wrapped: Vec<String> = wrap(text, self.term_width - 4)
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        for line in wrapped {
            let padding = (self.term_width - UnicodeWidthStr::width(line.as_str())) / 2;
            println!("{}{}", " ".repeat(padding), line.color(color));
        }
    }

    pub fn print_header(&self, text: &str) {
        println!("\n{}\n", "=".repeat(self.term_width).yellow());
        self.print_centered(text, Color::Cyan);
        println!("\n{}\n", "=".repeat(self.term_width).yellow());
    }

    pub fn print_footer(&self, text: &str) {
        println!("\n{}\n", "=".repeat(self.term_width).yellow());
        self.print_centered(text, Color::Cyan);
        println!("\n{}\n", "=".repeat(self.term_width).yellow());
    }

    pub fn print_separator(&self, color: Color) {
        println!("\n{}\n", "-".repeat(self.term_width).color(color));
    }

    pub fn print_wrapped(&self, text: &str, color: Color) {
        let wrapped_text = fill(text, self.term_width - 4);
        self.print_centered(&wrapped_text, color);
    }

    pub fn get_user_input(&self, prompt: &str) -> String {
        self.print_wrapped(prompt, Color::Yellow);
        print!(" >> ");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        input.trim().to_string()
    }

    pub fn print_thinking(&self) {
        print!("\n{}", "Thinking".yellow());
        io::stdout().flush().unwrap();
    }

    pub fn print_thinking_dot(&self) {
        print!(".");
        io::stdout().flush().unwrap();
    }

    pub fn clear_thinking(&self) {
        println!();
    }
}
