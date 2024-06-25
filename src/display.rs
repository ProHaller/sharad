use crate::settings::load_settings;
use colored::*;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
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
        let parser = Parser::new(&unescaped_text);
        let mut is_bold = false;
        let mut is_italic = false;
        let mut buffer = String::new();

        for event in parser {
            match event {
                Event::Start(tag) => match tag {
                    Tag::Emphasis => is_italic = true,
                    Tag::Strong => is_bold = true,
                    Tag::List(Some(number)) => {
                        buffer.push_str(&format!("{}. ", number));
                    }
                    _ => {}
                },
                Event::End(tag) => match tag {
                    TagEnd::Emphasis => is_italic = false,
                    TagEnd::Strong => is_bold = false,
                    _ => {}
                },
                Event::Text(text) => {
                    let mut styled_text = text.to_string();
                    if is_bold {
                        styled_text = styled_text.bold().to_string();
                    }
                    if is_italic {
                        styled_text = styled_text.italic().to_string();
                    }
                    buffer.push_str(&styled_text);
                }
                _ => {}
            }
        }

        let lines: Vec<&str> = buffer.lines().collect();
        for line in lines {
            let wrapped_lines = wrap(line, self.term_width - 4);
            for wrapped_line in wrapped_lines {
                let wrapped_line = wrapped_line.into_owned(); // Convert Cow<str> to String
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
