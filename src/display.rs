use crate::settings::load_settings;
use colored::*;
use std::io::{self, Write};
use textwrap::wrap;
use unicode_width::UnicodeWidthStr;

#[derive(Clone)]
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
            .map(|s| s.into_owned())
            .collect();
        for line in wrapped {
            let line_width = UnicodeWidthStr::width(line.as_str());
            let padding = if self.term_width > line_width {
                (self.term_width - line_width) / 2
            } else {
                0
            };
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
        let unescaped_text = unescape(text);
        let lines: Vec<&str> = unescaped_text.split('\n').collect();

        for line in lines {
            if line.trim().is_empty() {
                println!();
            } else {
                let formatted_line = self.apply_basic_formatting(line);
                let wrapped_lines = wrap(&formatted_line, self.term_width - 4);
                for wrapped_line in wrapped_lines {
                    let wrapped_line = wrapped_line.into_owned();
                    let line_width = UnicodeWidthStr::width(wrapped_line.as_str());
                    let padding = if self.term_width > line_width {
                        (self.term_width - line_width) / 2
                    } else {
                        0
                    };
                    println!("{}{}", " ".repeat(padding), wrapped_line.color(color));
                }
            }
        }
    }

    fn apply_basic_formatting(&self, line: &str) -> String {
        let mut result = String::new();
        let mut chars = line.chars().peekable();
        let mut is_bold = false;
        let mut is_italic = false;

        while let Some(ch) = chars.next() {
            match ch {
                '*' => {
                    if chars.peek() == Some(&'*') {
                        chars.next();
                        is_bold = !is_bold;
                        result.push_str(if is_bold { "\x1B[1m" } else { "\x1B[22m" });
                    } else {
                        is_italic = !is_italic;
                        result.push_str(if is_italic { "\x1B[3m" } else { "\x1B[23m" });
                    }
                }
                _ => result.push(ch),
            }
        }

        result
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

    pub fn print_debug(&self, text: &str, color: Color) {
        let settings = load_settings();
        if let Ok(debug) = settings {
            if debug.debug_mode {
                self.print_wrapped(text, color);
            }
        }
    }
}

fn unescape(s: &str) -> String {
    let mut unescaped = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => unescaped.push('\n'),
                Some('r') => unescaped.push('\r'),
                Some('t') => unescaped.push('\t'),
                Some('\\') => unescaped.push('\\'),
                Some('"') => unescaped.push('"'),
                Some(other) => {
                    unescaped.push('\\');
                    unescaped.push(other);
                }
                None => unescaped.push('\\'),
            }
        } else {
            unescaped.push(ch);
        }
    }
    unescaped
}
