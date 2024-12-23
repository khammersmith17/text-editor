use super::terminal::{Position, Size, Terminal};
use clipboard::{ClipboardContext, ClipboardProvider};
use crossterm::event::{read, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Instant;
mod buffer;
use super::editorcommands::{Direction, EditorCommand};
use buffer::Buffer;
use std::cmp::{max, min};
pub mod line;
mod theme;
use theme::Theme;
mod search;
use search::Search;
pub mod help;
use help::Help;
mod highlight;
use highlight::{Highlight, HighlightOp};

const PROGRAM_NAME: &str = env!("CARGO_PKG_NAME");
const PROGRAM_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct View {
    pub buffer: Buffer,
    pub needs_redraw: bool,
    pub size: Size,
    pub cursor_position: Position,
    pub screen_offset: Position,
    help_indicator: Help,
    search: Search,
    theme: Theme,
    clipboard: ClipboardContext,
    highlight: Highlight,
}

impl Default for View {
    fn default() -> Self {
        Self {
            buffer: Buffer::default(),
            needs_redraw: true,
            size: Terminal::size().unwrap_or_default(),
            cursor_position: Position::default(),
            screen_offset: Position::default(),
            help_indicator: Help::default(),
            search: Search::default(),
            theme: Theme::default(),
            clipboard: ClipboardProvider::new().unwrap(),
            highlight: Highlight::default(),
        }
    }
}

impl View {
    pub fn render(&mut self) {
        if (self.size.width == 0) | (self.size.height == 0) {
            return;
        }
        let screen_cut = if self.help_indicator.render_help | self.search.render_search {
            2
        } else {
            1
        };
        #[allow(clippy::integer_division)]
        for current_row in
            self.screen_offset.height..self.screen_offset.height + self.size.height - screen_cut
        {
            let relative_row = current_row - self.screen_offset.height;
            if self.search.render_search && self.search.line_indicies.contains(&current_row) {
                self.search.render_search_line(
                    current_row,
                    &self.buffer,
                    &self.screen_offset,
                    &self.size,
                    self.theme.search_highlight,
                    self.theme.search_text,
                );
                continue;
            }
            if self.highlight.render & self.highlight.map.contains_key(&current_row) {
                self.highlight.render_highlight_line(
                    &self.buffer.text[current_row].raw_string,
                    current_row,
                    self.screen_offset.width..self.screen_offset.width + self.size.width,
                    self.theme.search_highlight.clone(),
                    self.theme.search_text.clone(),
                );
                continue;
            }
            if let Some(line) = self.buffer.text.get(current_row) {
                self.render_line(
                    relative_row,
                    line.get_line_subset(
                        self.screen_offset.width
                            ..self.screen_offset.width.saturating_add(self.size.width),
                    ),
                );
            } else if self.buffer.is_empty() & (current_row == self.size.height / 3) {
                self.render_line(relative_row, &self.get_welcome_message());
            } else {
                self.render_line(relative_row, "~");
            }
        }
        if self.search.render_search {
            self.search.render_search_string(&self.size);
        }
        if self.help_indicator.render_help {
            self.render_help_line(self.size.height, self.size.width);
        }
        self.render_file_info(
            self.cursor_position.height - self.screen_offset.height + self.size.height,
        );

        self.needs_redraw = false;
    }

    fn render_help_line(&mut self, height: usize, width: usize) {
        if self.help_indicator.render_help
            & (Instant::now()
                .duration_since(self.help_indicator.time_began)
                .as_secs()
                < 5)
        {
            let mut render_message = format!(
                "HELP: {} | {} | {} | {} | {} | {}",
                "Ctrl-w = save",
                "Ctrl-q = quit",
                "Ctrl-j = jump-to",
                "Ctrl-f = search",
                "Ctrl-u = snap-up",
                "Ctrl-d = snap-down"
            );
            render_message.truncate(width);
            self.render_line(height.saturating_sub(2), &render_message);
        } else {
            self.help_indicator.render_help = false;
        }
    }

    fn render_file_info(&mut self, height: usize) {
        let saved = if !self.buffer.is_saved {
            "modified"
        } else {
            "saved"
        };
        let filename = match &self.buffer.filename {
            Some(file) => file,
            None => "-",
        };
        let render_message = if !self.buffer.is_empty() {
            format!(
                "Filename: {} | Status: {} | Line: {} / {}",
                filename,
                saved,
                self.cursor_position.height.saturating_add(1),
                self.buffer.len()
            )
        } else {
            format!("Filename: {} | Status: {} | Line: -", filename, saved)
        };

        self.render_line(height.saturating_sub(1), &render_message);
    }

    pub fn render_line<T: std::fmt::Display>(&self, row: usize, line: T) {
        let result = Terminal::render_line(row, line);
        debug_assert!(result.is_ok(), "Failed to render line")
    }
    pub fn resize(&mut self, size: Size) {
        self.size = size;
        self.handle_offset_screen_snap();
    }

    pub fn load(&mut self, filename: &str) {
        if let Ok(buffer) = Buffer::load(filename) {
            self.buffer = buffer;
            self.needs_redraw = true;
        }
    }

    fn get_welcome_message(&self) -> String {
        let mut welcome_message = format!("{PROGRAM_NAME} editor -- version {PROGRAM_VERSION}");
        let width = self.size.width;
        let len = welcome_message.len();
        #[allow(clippy::integer_division)]
        let padding = (width.saturating_sub(len)) / 2;

        let spaces = " ".repeat(padding.saturating_sub(1));
        welcome_message = format!("~{spaces}{welcome_message}");
        welcome_message.truncate(width);
        let range = self.screen_offset.width
            ..min(
                self.screen_offset.width.saturating_add(self.size.width),
                welcome_message.len(),
            );
        welcome_message = match welcome_message.get(range) {
            Some(text) => text.to_string(),
            None => "".to_string(),
        };
        welcome_message
    }

    pub fn move_cursor(&mut self, key_code: Direction) {
        if !self.buffer.is_empty() {
            let mut snap = false;
            match key_code {
                //if not on last line, move down
                //if the next line is shorter, snap to the end of that line
                Direction::Down => {
                    self.cursor_position.height = min(
                        self.cursor_position.height.saturating_add(1),
                        self.buffer.len().saturating_sub(1),
                    );
                    self.cursor_position.width = min(
                        self.cursor_position.width,
                        self.buffer
                            .text
                            .get(self.cursor_position.height)
                            .expect("Out of bounds error")
                            .grapheme_len(),
                    );
                }
                //if we are not in row 0, move up
                //if the line above is shorter than the previous line, snap to the end
                Direction::Up => {
                    self.cursor_position.height =
                        max(self.cursor_position.height.saturating_sub(1), 0);
                    self.cursor_position.width = min(
                        self.cursor_position.width,
                        self.buffer
                            .text
                            .get(self.cursor_position.height)
                            .expect("Out of bounds error")
                            .grapheme_len(),
                    );
                }
                //move left
                //if we are at 0,0 no action
                //if we are at width 0, snap to the right end of the previous line
                //else move left 1
                Direction::Left => {
                    if (self.cursor_position.width == 0) & (self.cursor_position.height > 0) {
                        self.cursor_position.height =
                            max(self.cursor_position.height.saturating_sub(1), 0);
                        self.cursor_position.width = self
                            .buffer
                            .text
                            .get(self.cursor_position.height)
                            .expect("Out of bounds error")
                            .grapheme_len();
                        snap = true;
                    } else {
                        self.cursor_position.width = self.cursor_position.width.saturating_sub(1);
                    }
                }
                //if we are on the last line at the -1 position of the text, do nothing
                //if we are at the end of the line, snap to position 0 on the next line
                //else move right 1 char
                Direction::Right => {
                    let grapheme_len = self
                        .buffer
                        .text
                        .get(self.cursor_position.height)
                        .expect("Out of bounds error")
                        .grapheme_len();

                    let text_height = self.buffer.len().saturating_sub(1);

                    if (self.cursor_position.width == grapheme_len)
                        & (self.cursor_position.height < text_height)
                    {
                        self.cursor_position.height = self.cursor_position.height.saturating_add(1);
                        self.cursor_position.width = 0;
                        snap = true;
                    } else {
                        self.cursor_position.width =
                            min(self.cursor_position.width.saturating_add(1), grapheme_len);
                    }
                }
                //move to last line, cursor width will stay the same
                Direction::PageDown => {
                    self.cursor_position.height = self.buffer.len().saturating_sub(1);
                    snap = true;
                }
                //move to the first line, cursor width stays the same
                Direction::PageUp => {
                    self.cursor_position.height = 0;
                    snap = true;
                }
                //move to end of current line
                Direction::End => {
                    self.cursor_position.width = self
                        .buffer
                        .text
                        .get(self.cursor_position.height)
                        .expect("index Error")
                        .grapheme_len();
                    snap = true;
                }
                //move to start of current line
                Direction::Home => {
                    self.cursor_position.width = 0;
                    snap = true;
                }
            }
            if snap {
                self.handle_offset_screen_snap();
            } else {
                self.update_offset_single_move();
            }
        } else {
            self.cursor_position.width = 0;
            self.cursor_position.height = 0;
        }
        self.needs_redraw = true;
    }

    fn insert_char(&mut self, insert_char: char) {
        self.buffer
            .update_line_insert(&mut self.cursor_position, insert_char);

        self.buffer.is_saved = false;
    }

    fn insert_tab(&mut self) {
        self.buffer.insert_tab(&self.cursor_position);
        self.cursor_position.width = self.cursor_position.width.saturating_add(4);
    }

    fn delete_char(&mut self) {
        //get the width of the char being deleted to update the cursor position
        let removed_char_width = self.buffer.update_line_delete(&self.cursor_position);

        self.cursor_position.width = self
            .cursor_position
            .width
            .saturating_sub(removed_char_width);
    }

    pub fn get_file_name(&mut self) {
        // clear_screen and render screen to get file name
        let mut filename_buffer = String::new();
        let mut curr_position: usize = 10;
        self.render_filename_screen(&filename_buffer, curr_position);
        loop {
            match read() {
                Ok(event) => {
                    match event {
                        Event::Key(KeyEvent { code, .. }) => match code {
                            KeyCode::Char(letter) => {
                                filename_buffer.push(letter);
                                curr_position += 1;
                            }
                            KeyCode::Backspace => {
                                filename_buffer.pop();
                                curr_position = std::cmp::max(10, curr_position.saturating_sub(1));
                            }
                            KeyCode::Enter => break,
                            _ => {
                                //skipping all other keycode events
                            }
                        },
                        _ => {
                            //skipping all other events
                        }
                    }
                }

                Err(err) => {
                    #[cfg(debug_assertions)]
                    {
                        panic!("Could not handle event: {err}");
                    }
                }
            }
            self.render_filename_screen(&filename_buffer, curr_position);
        }

        self.buffer.assume_file_name(filename_buffer);
        self.needs_redraw = true;
    }

    fn render_filename_screen(&self, curr_filename: &str, curr_position: usize) {
        Terminal::hide_cursor().expect("Error hiding cursor");
        Terminal::move_cursor_to(Position {
            height: 0,
            width: 0,
        })
        .expect("Error moving cursor to start");
        Terminal::clear_screen().expect("Error clearing screen");
        self.render_line(0, &format!("Filename: {}", &curr_filename));
        Terminal::move_cursor_to(Position {
            height: 0,
            width: curr_position,
        })
        .expect("Error moving cursor");
        Terminal::show_cursor().expect("Error showing cursor");
        Terminal::execute().expect("Error flushing std buffer");
    }

    pub fn handle_event(&mut self, command: EditorCommand) {
        //match the event to the enum value and handle the event accrodingly
        match command {
            EditorCommand::Move(direction) => self.move_cursor(direction),
            EditorCommand::JumpWord(direction) => self.jump_word(direction),
            EditorCommand::Resize(size) => {
                self.resize(size);
            }
            EditorCommand::Save => {
                if self.buffer.filename.is_none() {
                    self.get_file_name();
                }
                self.buffer.save();
            }
            EditorCommand::Theme => {
                self.theme.set_theme();
            }
            EditorCommand::Paste => {
                //buffer method to add text to the buffer
                let paste_text = self.clipboard.get_contents().unwrap();
                self.buffer
                    .add_text_from_clipboard(paste_text, &mut self.cursor_position);
            }
            EditorCommand::Highlight => {
                self.highlight.render = true;
                self.handle_highlight();
            }
            EditorCommand::Search => {
                if self.help_indicator.render_help {
                    self.help_indicator.render_help = false;
                }
                self.search.render_search = true;
                self.handle_search();
            }
            EditorCommand::Insert(char) => {
                self.insert_char(char);
                self.update_offset_single_move();
            }
            EditorCommand::Tab => self.insert_tab(),
            EditorCommand::JumpLine => self.jump_cursor(),
            EditorCommand::Delete => {
                //todo add logic for when a line is empty
                match self.cursor_position.width {
                    0 => {
                        if self.cursor_position.height == 0 {
                        } else if self
                            .buffer
                            .text
                            .get(self.cursor_position.height)
                            .expect("Out of bounds error")
                            .is_empty()
                        {
                            self.buffer.text.remove(self.cursor_position.height);
                            self.cursor_position.height =
                                self.cursor_position.height.saturating_sub(1);
                            self.cursor_position.width = self
                                .buffer
                                .text
                                .get(self.cursor_position.height)
                                .expect("Out of bounds error")
                                .grapheme_len();
                            self.handle_offset_screen_snap();
                        } else {
                            let new_width = self
                                .buffer
                                .text
                                .get(self.cursor_position.height.saturating_sub(1))
                                .expect("Out of bounds error")
                                .grapheme_len();
                            self.buffer.join_line(self.cursor_position.height);
                            self.cursor_position.height =
                                self.cursor_position.height.saturating_sub(1);
                            self.cursor_position.width = new_width;
                        }
                    }
                    _ => {
                        self.delete_char();
                    }
                };
            }
            EditorCommand::NewLine => {
                let grapheme_len = if !self.buffer.is_empty() {
                    self.buffer
                        .text
                        .get(self.cursor_position.height)
                        .expect("Out of bounds error")
                        .grapheme_len()
                } else {
                    0
                };

                if self.cursor_position.width == grapheme_len {
                    self.buffer.new_line(self.cursor_position.height);
                } else {
                    self.buffer
                        .split_line(self.cursor_position.height, self.cursor_position.width);
                }

                self.cursor_position.height = self.cursor_position.height.saturating_add(1);
                self.cursor_position.width = if self.buffer.is_tab(&Position {
                    height: self.cursor_position.height,
                    width: 4,
                }) {
                    4
                } else {
                    0
                };
                self.handle_offset_screen_snap();
            }
            EditorCommand::Help => {
                self.help_indicator.render_help = true;
                self.help_indicator.time_began = Instant::now();
            }
            _ => {}
        }
        self.needs_redraw = true;
    }

    fn jump_cursor(&mut self) {
        let neg_2 = self.size.height.saturating_sub(2);
        let render_string: String = "Jump to: ".into();
        let mut line = 0_usize;
        Terminal::move_cursor_to(Position {
            height: neg_2,
            width: 0,
        })
        .expect("Error moving cursor");
        let _ = Terminal::render_line(neg_2, format!("{}", render_string));

        loop {
            match read() {
                Ok(event) => match event {
                    Event::Key(KeyEvent { code, .. }) => match code {
                        KeyCode::Char(val) => {
                            if let Some(digit) = val.to_digit(10) {
                                line = line * 10 + digit as usize;
                            }
                        }
                        KeyCode::Backspace => {
                            line = if line > 9 { line / 10 } else { 0 };
                        }
                        KeyCode::Enter => {
                            // if line > buffer.len(), give buffer len
                            if line < self.buffer.len() {
                                self.cursor_position.height = line.saturating_sub(1);
                            } else {
                                self.move_cursor(Direction::PageDown);
                            };

                            if (self.cursor_position.height
                                > self.size.height + self.screen_offset.height)
                                | (self.cursor_position.height < self.screen_offset.height)
                            {
                                self.handle_offset_screen_snap();
                            }
                            break;
                        }
                        KeyCode::Esc => {
                            break;
                        }
                        _ => {}
                    },
                    _ => {}
                },
                Err(_) => {}
            }
            match line {
                0 => {
                    let _ = Terminal::render_line(neg_2, &format!("{}", render_string));
                }
                _ => {
                    let _ = Terminal::render_line(neg_2, &format!("{}{}", render_string, line));
                }
            }
            let _ = Terminal::execute();
        }
    }

    fn handle_offset_screen_snap(&mut self) {
        // updates the offset when offset adjustment is > 1
        if self.cursor_position.height.saturating_add(1)
            >= self.size.height + self.screen_offset.height
        {
            self.screen_offset.height = min(
                self.buffer
                    .text
                    .len()
                    .saturating_sub(self.size.height)
                    .saturating_add(2), // leave space for the file info line
                self.cursor_position
                    .height
                    .saturating_sub(self.size.height)
                    .saturating_add(2),
            );
            if self.search.render_search | self.help_indicator.render_help {
                self.screen_offset.height = self.screen_offset.height.saturating_add(1);
            }
        } else if self.cursor_position.height < self.screen_offset.height {
            self.screen_offset.height = self.cursor_position.height.saturating_sub(1);
        }

        if self.cursor_position.height == 0 {
            self.screen_offset.height = 0;
        }

        if self.cursor_position.width == 0 {
            self.screen_offset.width = 0;
        }

        if self.cursor_position.width >= self.size.width + self.screen_offset.width {
            self.screen_offset.width = self
                .cursor_position
                .width
                .saturating_sub(self.size.width)
                .saturating_add(1);
        } else if self.cursor_position.width < self.screen_offset.width {
            self.screen_offset.width = self.cursor_position.width.saturating_sub(1);
        }
    }
    fn update_offset_single_move(&mut self) {
        //if cursor moves beyond height + offset -> increment height offset
        if self.cursor_position.height
            >= (self.size.height + self.screen_offset.height).saturating_sub(1)
        {
            self.screen_offset.height = min(
                self.screen_offset.height.saturating_add(1),
                self.cursor_position
                    .height
                    .saturating_sub(self.size.height)
                    .saturating_add(2), // space for file info line
            );
        }
        // if height moves less than the offset -> decrement height
        if self.cursor_position.height <= self.screen_offset.height {
            self.screen_offset.height = self.cursor_position.height;
        }
        //if widith less than offset -> decerement width
        if self.cursor_position.width < self.screen_offset.width {
            self.screen_offset.width = self.cursor_position.width;
        }
        // if new position is greater than offset, offset gets current_width - screen width
        // this better handles snapping the cursor to the end of the line
        if self.cursor_position.width >= self.size.width + self.screen_offset.width {
            //self.screen_offset.width = self.screen_offset.width.saturating_sub(1);
            self.screen_offset.width = self.screen_offset.width.saturating_add(1);
        }
    }

    fn handle_search(&mut self) {
        self.search.previous_position = self.cursor_position.clone();
        self.search.previous_offset = self.screen_offset.clone();

        self.search.string.clear();
        self.search.search_index = 0;

        // keep a stack of search positions so we only need to compute the positions when the user
        // adds to a search string
        // when the user removes from the search string
        // we pop the stack

        loop {
            self.render_search();
            match read() {
                Ok(event) => {
                    match event {
                        Event::Key(KeyEvent {
                            code, modifiers, ..
                        }) => match (code, modifiers) {
                            (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                                if !self.search.stack.is_empty() {
                                    let curr_results =
                                        self.search.stack.get(self.search.stack.len() - 1).unwrap();
                                    self.search.search_index =
                                        if curr_results.len().saturating_sub(1)
                                            > self.search.search_index
                                        {
                                            self.search.search_index.saturating_add(1)
                                        } else {
                                            0
                                        };
                                }
                            }
                            (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                                if !self.search.stack.is_empty() {
                                    let curr_results =
                                        self.search.stack.get(self.search.stack.len() - 1).unwrap();
                                    self.search.search_index = if self.search.search_index > 0 {
                                        self.search.search_index.saturating_sub(1)
                                    } else {
                                        curr_results.len().saturating_sub(1)
                                    };
                                }
                            }
                            (KeyCode::Char(char), _) => {
                                self.search.string.push(char);
                                self.search
                                    .stack
                                    .push(self.buffer.search(&self.search.string));
                                self.search.search_index = match self
                                    .search
                                    .find_relative_start(&self.search.previous_position.height)
                                {
                                    Some(ind) => ind,
                                    None => 0,
                                };
                                self.search.set_line_indicies();
                            }
                            (KeyCode::Backspace, _) => {
                                if !self.search.string.is_empty() {
                                    self.search.string.pop();
                                    self.search.stack.pop();
                                    self.search.search_index = match self
                                        .search
                                        .find_relative_start(&self.search.previous_position.height)
                                    {
                                        Some(ind) => ind,
                                        None => 0,
                                    };
                                    self.search.set_line_indicies();
                                }
                            }
                            (KeyCode::Esc, _) => {
                                //return to pre search screen state
                                self.revert_screen_state();
                                self.search.clean_up_search();
                                break;
                            }
                            (KeyCode::Enter, _) => {
                                //assume current state on screen after search
                                self.search.clean_up_search();
                                break;
                            }
                            _ => {}
                        },
                        Event::Resize(width_u16, height_u16) => self.resize(Size {
                            height: height_u16 as usize,
                            width: width_u16 as usize,
                        }),
                        _ => {
                            //not addressing other events
                        }
                    }
                }
                Err(_) => {}
            }

            if self.search.stack.is_empty() {
                self.revert_screen_state();
                continue;
            }

            if self
                .search
                .stack
                .get(self.search.stack.len() - 1)
                .unwrap()
                .is_empty()
            {
                self.revert_screen_state();
                continue;
            }

            //grab the latest search results from the stack
            //get the search index position
            self.cursor_position = self
                .search
                .stack
                .get(self.search.stack.len() - 1)
                .expect("Search stack empty")
                .get(self.search.search_index)
                .expect("Out of bounds")
                .clone();

            // if the search position is out of current screen bounds
            if (self.cursor_position.height
                > self.screen_offset.height + self.size.height.saturating_sub(2))
                | (self.cursor_position.height < self.screen_offset.height)
            {
                self.handle_offset_screen_snap();
            }
        }
        self.search.render_search = false;
        self.render();
    }

    fn revert_screen_state(&mut self) {
        self.cursor_position = self.search.previous_position;
        self.screen_offset = self.search.previous_offset;
    }

    fn render_search(&mut self) {
        // this largely is the same logic as Editor::refresh_screen
        // maybe that logic should be called out of view to not reproduce code
        Terminal::hide_cursor().unwrap();
        Terminal::move_cursor_to(self.screen_offset).unwrap();
        Terminal::clear_screen().unwrap();
        self.render();
        Terminal::move_cursor_to(Position {
            height: self
                .cursor_position
                .height
                .saturating_sub(self.screen_offset.height),
            width: self.cursor_position.width,
        })
        .unwrap();
        Terminal::show_cursor().unwrap();
        Terminal::execute().unwrap();
    }

    fn jump_word(&mut self, dir: Direction) {
        match dir {
            Direction::Right => self.buffer.find_next_word(&mut self.cursor_position),
            Direction::Left => self.buffer.find_prev_word(&mut self.cursor_position),
            _ => {} //direction should only be left or right at this point
        };
    }

    fn handle_highlight(&mut self) {
        let mut end = self.cursor_position.clone();
        let max_height = self.buffer.len() - 1;

        loop {
            match read() {
                Ok(event) => match event {
                    Event::Key(KeyEvent {
                        code, modifiers, ..
                    }) => match (code, modifiers) {
                        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                            self.highlight.render = false;
                            break;
                        }
                        (KeyCode::Right, _) => {
                            if end.width
                                == self.buffer.text[end.height]
                                    .grapheme_len()
                                    .saturating_sub(1)
                            {
                                end.height = std::cmp::min(end.height + 1, max_height);
                                end.width = 0;
                                self.highlight.update_map(&end, HighlightOp::OverflowRight);
                            } else {
                                end.width += 1;
                                self.highlight.update_map(&end, HighlightOp::Right);
                            }
                        }
                        (KeyCode::Left, _) => {
                            if end.width == 0 {
                                end.height = end.height.saturating_sub(1);
                                end.width = self.buffer.text[end.height]
                                    .grapheme_len()
                                    .saturating_sub(1);
                                self.highlight.update_map(&end, HighlightOp::OverflowLeft);
                            } else {
                                end.width -= 1;
                                self.highlight.update_map(&end, HighlightOp::Left);
                            }
                        }
                        (KeyCode::Down, _) => {
                            if end.height == self.buffer.len() {
                                continue;
                            }
                            end.height += 1;
                            end.width = std::cmp::min(
                                end.width,
                                self.buffer.text[end.height]
                                    .grapheme_len()
                                    .saturating_sub(1),
                            );
                            self.highlight.update_map(&end, HighlightOp::Down);
                        }
                        (KeyCode::Up, _) => {
                            if end.height == 0 {
                                continue;
                            }
                            end.height = end.height.saturating_sub(1);
                            end.width = std::cmp::min(
                                end.width,
                                self.buffer.text[end.height]
                                    .grapheme_len()
                                    .saturating_sub(1),
                            );
                            self.highlight.update_map(&end, HighlightOp::Up);
                        }
                        (KeyCode::Esc, _) => {
                            self.highlight.render = false;
                            return;
                        }
                        _ => {}
                    },
                    Event::Resize(width_u16, height_u16) => self.resize(Size {
                        height: height_u16 as usize,
                        width: width_u16 as usize,
                    }),
                    _ => {}
                },
                _ => {}
            }
            //TODO:
            //save these in a hashmap pick from the hashmap in highlight
            //render the screen every move
            //self.highlight.update_map();
            self.highlight
                .resolve_orientation(&self.cursor_position, &end);
            Terminal::hide_cursor().unwrap();
            self.render();
            Terminal::move_cursor_to(end).unwrap();
            Terminal::show_cursor().unwrap();
            Terminal::execute().unwrap();
        }

        let copy_string = if end.height > self.cursor_position.height {
            Highlight::generate_copy_str(&self.buffer, &self.cursor_position, &end)
        } else {
            Highlight::generate_copy_str(&self.buffer, &end, &self.cursor_position)
        };
        if copy_string.is_empty() {
            return;
        }
        Highlight::copy_text_to_clipboard(&mut self.clipboard, copy_string);
    }
}
