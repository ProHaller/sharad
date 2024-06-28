use crate::settings::load_settings;
use std::io::Write;
use textwrap::wrap;
use unicode_width::UnicodeWidthStr;

use copypasta::{ClipboardContext, ClipboardProvider};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    style::{Color, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType, ScrollUp},
};
use std::io;
use std::io::stdout;
use std::time::Duration;

#[derive(Clone)]
pub struct Display {
    term_width: usize,
    term_height: usize,
}

impl Display {
    pub fn new() -> Self {
        let (width, height) = terminal::size().unwrap_or((80, 24));
        Display {
            term_width: width as usize,
            term_height: height as usize,
        }
    }

    pub fn update_dimensions(&mut self) {
        let (width, height) = terminal::size().unwrap_or((80, 24));
        self.term_width = width as usize;
        self.term_height = height as usize;
    }

    pub fn get_user_input(&mut self, prompt: &str) -> Result<Option<String>, io::Error> {
        self.update_dimensions();
        terminal::enable_raw_mode()?;

        let wrapped_prompt: Vec<String> = wrap(prompt, self.term_width.saturating_sub(4))
            .into_iter()
            .map(|s| s.into_owned())
            .collect();
        let prompt_lines = wrapped_prompt.len();

        let prompt_y = self.ensure_space_for_lines(prompt_lines + 2);

        for (i, line) in wrapped_prompt.iter().enumerate() {
            execute!(
                io::stdout(),
                cursor::MoveTo(0, prompt_y + i as u16),
                SetForegroundColor(Color::Yellow)
            )?;
            println!("{}", line);
        }

        execute!(
            io::stdout(),
            cursor::MoveTo(0, prompt_y + prompt_lines as u16),
            SetForegroundColor(Color::Yellow)
        )?;
        print!(" >> ");
        execute!(io::stdout(), ResetColor)?;
        io::stdout().flush()?;

        let mut input: Vec<char> = Vec::new();
        let mut cursor_position = 0;

        let mut clipboard = ClipboardContext::new().unwrap();

        loop {
            self.redraw_input(
                &input.iter().collect::<String>(),
                cursor_position,
                prompt_y,
                prompt_lines,
            )?;

            if event::poll(Duration::from_millis(10))? {
                if let Event::Key(KeyEvent {
                    code,
                    modifiers,
                    kind: KeyEventKind::Press,
                    ..
                }) = event::read()?
                {
                    match code {
                        KeyCode::Esc => {
                            terminal::disable_raw_mode()?;
                            return Ok(None);
                        }
                        KeyCode::Enter => {
                            terminal::disable_raw_mode()?;
                            return Ok(Some(input.iter().collect::<String>().trim().to_string()));
                        }
                        KeyCode::Char('v') if modifiers == KeyModifiers::CONTROL => {
                            if let Ok(clipboard_contents) = clipboard.get_contents() {
                                for c in clipboard_contents.chars() {
                                    input.insert(cursor_position, c);
                                    cursor_position += 1;
                                }
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

    fn redraw_input(
        &mut self,
        input: &str,
        cursor_position: usize,
        prompt_y: u16,
        prompt_lines: usize,
    ) -> io::Result<()> {
        execute!(
            io::stdout(),
            cursor::MoveTo(0, prompt_y + prompt_lines as u16),
            Clear(ClearType::CurrentLine)
        )?;
        print!(" >> {}", input);
        self.move_cursor(cursor_position, prompt_y, prompt_lines)?;
        io::stdout().flush()
    }

    fn move_cursor(
        &mut self,
        cursor_position: usize,
        prompt_y: u16,
        prompt_lines: usize,
    ) -> io::Result<()> {
        execute!(
            io::stdout(),
            cursor::MoveTo((cursor_position + 4) as u16, prompt_y + prompt_lines as u16)
        )
    }

    pub fn print_thinking(&mut self) {
        let (_, cursor_y) = self.get_current_cursor_position();
        if execute!(
            stdout(),
            cursor::MoveTo(0, cursor_y),
            SetForegroundColor(Color::Yellow)
        )
        .is_ok()
        {
            print!("\nThinking");
            let _ = execute!(stdout(), ResetColor);
            let _ = stdout().flush();
        }
    }

    pub fn print_thinking_dot(&mut self) {
        if execute!(stdout(), SetForegroundColor(Color::Yellow)).is_ok() {
            print!(".");
            let _ = execute!(stdout(), ResetColor);
            let _ = stdout().flush();
        }
    }

    pub fn clear_thinking(&self) {
        println!();
    }

    pub fn print_debug(&mut self, text: &str, color: Color) {
        if let Ok(settings) = load_settings() {
            if settings.debug_mode {
                self.print_wrapped(text, color);
            }
        }
    }

    fn get_current_cursor_position(&self) -> (u16, u16) {
        cursor::position().unwrap_or((0, 0))
    }

    fn ensure_space_for_lines(&self, lines_needed: usize) -> u16 {
        let (_, cursor_y) = self.get_current_cursor_position();
        let available_lines = self.term_height.saturating_sub(cursor_y as usize);

        if lines_needed > available_lines {
            let lines_to_scroll = lines_needed.saturating_sub(available_lines);
            if execute!(stdout(), ScrollUp(lines_to_scroll as u16)).is_err() {
                return cursor_y; // return current cursor position if scroll fails
            }
            self.term_height.saturating_sub(lines_needed) as u16
        } else {
            cursor_y
        }
    }

    pub fn print_centered(&mut self, text: &str, color: Color) {
        self.update_dimensions();
        let wrapped: Vec<String> = wrap(text, self.term_width.saturating_sub(4))
            .into_iter()
            .map(|s| s.into_owned())
            .collect();
        let start_y = self.ensure_space_for_lines(wrapped.len());
        for (i, line) in wrapped.iter().enumerate() {
            let line_width = UnicodeWidthStr::width(line.as_str());
            let padding = self.term_width.saturating_sub(line_width) / 2;
            if execute!(
                stdout(),
                cursor::MoveTo(padding as u16, start_y + i as u16),
                SetForegroundColor(color)
            )
            .is_ok()
            {
                print!("{}", line);
                let _ = execute!(stdout(), ResetColor);
            }
        }
        let _ = execute!(stdout(), cursor::MoveToNextLine(1));
    }

    pub fn print_header(&mut self, text: &str) {
        self.update_dimensions();
        self.print_separator(Color::Yellow);
        self.print_centered(text, Color::Cyan);
        self.print_separator(Color::Yellow);
    }

    pub fn print_footer(&mut self, text: &str) {
        self.update_dimensions();
        self.print_separator(Color::Yellow);
        self.print_centered(text, Color::Cyan);
        self.print_separator(Color::Yellow);
    }

    pub fn print_separator(&mut self, color: Color) {
        self.update_dimensions();
        let (_, cursor_y) = self.get_current_cursor_position();
        if execute!(
            stdout(),
            cursor::MoveTo(0, cursor_y),
            SetForegroundColor(color)
        )
        .is_ok()
        {
            println!("{}", "=".repeat(self.term_width));
            let _ = execute!(stdout(), ResetColor);
        }
    }

    pub fn print_wrapped(&mut self, text: &str, color: Color) {
        self.update_dimensions();
        let unescaped_text = unescape(text);
        let lines: Vec<&str> = unescaped_text.split('\n').collect();
        let mut total_lines = 0;

        for line in lines.iter() {
            if line.trim().is_empty() {
                total_lines += 1;
            } else {
                let formatted_line = self.apply_basic_formatting(line);
                let wrapped_lines = wrap(&formatted_line, self.term_width.saturating_sub(4));
                total_lines += wrapped_lines.len();
            }
        }

        let start_y = self.ensure_space_for_lines(total_lines);
        let mut current_y = start_y;

        for line in lines {
            if line.trim().is_empty() {
                current_y += 1;
            } else {
                let formatted_line = self.apply_basic_formatting(line);
                let wrapped_lines = wrap(&formatted_line, self.term_width.saturating_sub(4));
                for wrapped_line in wrapped_lines {
                    let wrapped_line = wrapped_line.into_owned();
                    let line_width = UnicodeWidthStr::width(wrapped_line.as_str());
                    let padding = self.term_width.saturating_sub(line_width) / 2;
                    if execute!(
                        stdout(),
                        cursor::MoveTo(padding as u16, current_y),
                        SetForegroundColor(color)
                    )
                    .is_ok()
                    {
                        println!("{}", wrapped_line);
                        let _ = execute!(stdout(), ResetColor);
                    }
                    current_y += 1;
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
