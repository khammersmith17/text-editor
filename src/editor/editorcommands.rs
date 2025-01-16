use super::terminal::Size;
use super::terminal::{Coordinate, Position};
use super::view::buffer::Buffer;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use std::convert::TryFrom;

#[derive(Copy, Clone, Debug)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    End,
    Home,
}

impl Direction {
    // allows single source of cursor movement across modes
    pub fn move_cursor(&self, cursor_position: &mut Position, buffer: &Buffer) {
        match *self {
            //if not on last line, move down
            //if the next line is shorter, snap to the end of that line
            Direction::Down => {
                cursor_position.down(1, buffer.len().saturating_sub(1));
                cursor_position.resolve_width(buffer.text[cursor_position.height].grapheme_len());
            }
            //if we are not in row 0, move up
            //if the line above is shorter than the previous line, snap to the end
            Direction::Up => {
                cursor_position.up(1);
                cursor_position.resolve_width(buffer.text[cursor_position.height].grapheme_len());
            }
            //move left
            //if we are at 0,0 no action
            //if we are at width 0, snap to the right end of the previous line
            //else move left 1
            Direction::Left => match (cursor_position.at_left_edge(), cursor_position.at_top()) {
                (true, false) => {
                    cursor_position.up(1);
                    cursor_position.snap_right(buffer.text[cursor_position.height].grapheme_len());
                }
                _ => {
                    cursor_position.left(1);
                }
            },
            //if we are on the last line at the -1 position of the text, do nothing
            //if we are at the end of the line, snap to position 0 on the next line
            //else move right 1 char
            Direction::Right => {
                let grapheme_len = buffer.text[cursor_position.height].grapheme_len();
                let text_height = buffer.len().saturating_sub(1);

                match (
                    cursor_position.at_max_width(grapheme_len),
                    cursor_position.at_max_height(text_height),
                ) {
                    (true, false) => {
                        cursor_position.down(1, text_height);
                        cursor_position.snap_left();
                    }
                    _ => cursor_position.right(1, grapheme_len),
                };
            }
            //move to last line, cursor width will stay the same
            Direction::PageDown => {
                cursor_position.page_down(buffer.len().saturating_sub(1));
            }
            //move to the first line, cursor width stays the same
            Direction::PageUp => {
                cursor_position.page_up();
            }
            //move to end of current line
            Direction::End => {
                cursor_position.snap_right(buffer.text[cursor_position.height].grapheme_len());
            }
            //move to start of current line
            Direction::Home => {
                cursor_position.snap_left();
            }
        }
    }
}

#[derive(Copy, Clone)]
pub enum EditorCommand {
    Move(Direction),
    Insert(char),
    Resize(Size),
    JumpWord(Direction),
    JumpLine,
    Highlight,
    Paste,
    Tab,
    NewLine,
    Save,
    Theme,
    Delete,
    VimMode,
    Search,
    Help,
    None,
    Quit,
}

impl TryFrom<Event> for EditorCommand {
    type Error = String;
    fn try_from(event: Event) -> Result<Self, Self::Error> {
        match event {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => match (code, modifiers) {
                (KeyCode::Char('q'), KeyModifiers::CONTROL) => Ok(Self::Quit),
                (KeyCode::Char('j'), KeyModifiers::CONTROL) => Ok(Self::JumpLine),
                (KeyCode::Char('l'), KeyModifiers::CONTROL) => Ok(Self::Move(Direction::Home)),
                (KeyCode::Char('u'), KeyModifiers::CONTROL) => Ok(Self::Move(Direction::PageUp)),
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => Ok(Self::Move(Direction::PageDown)),
                (KeyCode::Char('r'), KeyModifiers::CONTROL) => Ok(Self::Move(Direction::End)),
                (KeyCode::Char('w'), KeyModifiers::CONTROL) => Ok(Self::Save),
                (KeyCode::Char('h'), KeyModifiers::CONTROL) => Ok(Self::Help),
                (KeyCode::Char('f'), KeyModifiers::CONTROL) => Ok(Self::Search),
                (KeyCode::Char('t'), KeyModifiers::CONTROL) => Ok(Self::Theme),
                (KeyCode::Char('v'), KeyModifiers::CONTROL) => Ok(Self::Paste),
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => Ok(Self::Highlight),
                (KeyCode::Char('n'), KeyModifiers::CONTROL) => Ok(Self::VimMode),
                (KeyCode::Left, KeyModifiers::SHIFT) => Ok(Self::JumpWord(Direction::Left)),
                (KeyCode::Right, KeyModifiers::SHIFT) => Ok(Self::JumpWord(Direction::Right)),
                (KeyCode::Up, _) => Ok(Self::Move(Direction::Up)),
                (KeyCode::Down, _) => Ok(Self::Move(Direction::Down)),
                (KeyCode::Left, _) => Ok(Self::Move(Direction::Left)),
                (KeyCode::Right, _) => Ok(Self::Move(Direction::Right)),
                (KeyCode::Char(c), _) => Ok(Self::Insert(c)),
                (KeyCode::Backspace, _) => Ok(Self::Delete),
                (KeyCode::Enter, _) => Ok(Self::NewLine),
                (KeyCode::Tab, _) => Ok(Self::Tab),
                _ => Ok(Self::None),
            },
            Event::Resize(width_16, height_u16) => {
                #[allow(clippy::as_conversions)]
                let height = height_u16 as usize;
                #[allow(clippy::as_conversions)]
                let width = width_16 as usize;
                Ok(Self::Resize(Size { height, width }))
            }
            _ => Err(format!("Event not supported {event:?}")),
        }
    }
}

#[derive(Copy, Clone)]
pub enum SearchCommand {
    Insert(char),
    Next,
    Previous,
    BackSpace,
    RevertState,
    AssumeState,
    Resize(Size),
    NoAction,
}

impl TryFrom<Event> for SearchCommand {
    type Error = String;
    fn try_from(event: Event) -> Result<Self, Self::Error> {
        match event {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => match (code, modifiers) {
                (KeyCode::Char('n'), KeyModifiers::CONTROL) => Ok(Self::Next),
                (KeyCode::Char('p'), KeyModifiers::CONTROL) => Ok(Self::Previous),
                (_, KeyModifiers::CONTROL) => Ok(Self::NoAction),
                (KeyCode::Char(c), _) => Ok(Self::Insert(c)),
                (KeyCode::Enter, _) => Ok(Self::AssumeState),
                (KeyCode::Esc, _) => Ok(Self::RevertState),
                (KeyCode::Backspace, _) => Ok(Self::BackSpace),
                _ => Ok(Self::NoAction),
            },
            #[allow(clippy::as_conversions)]
            Event::Resize(width_u16, height_u16) => Ok(Self::Resize(Size {
                height: height_u16 as usize,
                width: width_u16 as usize,
            })),
            _ => Err("Invalid key press read".into()),
        }
    }
}

pub enum HighlightCommand {
    RevertState,
    Copy,
    Resize(Size),
    Move(Direction),
    NoAction,
    Delete,
}

impl TryFrom<Event> for HighlightCommand {
    type Error = String;
    fn try_from(event: Event) -> Result<Self, Self::Error> {
        match event {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => match (code, modifiers) {
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => Ok(Self::Copy),
                (_, KeyModifiers::CONTROL) => Ok(Self::NoAction),
                (KeyCode::Up, _) => Ok(Self::Move(Direction::Up)),
                (KeyCode::Down, _) => Ok(Self::Move(Direction::Down)),
                (KeyCode::Right, _) => Ok(Self::Move(Direction::Right)),
                (KeyCode::Left, _) => Ok(Self::Move(Direction::Left)),
                (KeyCode::Esc, _) => Ok(Self::RevertState),
                (KeyCode::Backspace, _) => Ok(Self::Delete),
                _ => Ok(Self::NoAction),
            },
            #[allow(clippy::as_conversions)]
            Event::Resize(width_u16, height_u16) => Ok(Self::Resize(Size {
                height: height_u16 as usize,
                width: width_u16 as usize,
            })),
            _ => Err("Invalid key press read".into()),
        }
    }
}

pub enum FileNameCommand {
    Insert(char),
    BackSpace,
    SaveFileName,
    Quit,
    NoAction,
}

impl TryFrom<Event> for FileNameCommand {
    type Error = String;
    fn try_from(event: Event) -> Result<Self, Self::Error> {
        match event {
            Event::Key(KeyEvent { code, .. }) => match code {
                KeyCode::Char(c) => Ok(Self::Insert(c)),
                KeyCode::Backspace => Ok(Self::BackSpace),
                KeyCode::Enter => Ok(Self::SaveFileName),
                KeyCode::Esc => Ok(Self::Quit),
                _ => Ok(Self::NoAction),
            },
            _ => Ok(Self::NoAction),
        }
    }
}

pub enum VimModeCommands {
    Move(Direction),
    NoAction,
    Resize(Size),
    Exit,
}

impl TryFrom<Event> for VimModeCommands {
    type Error = String;
    fn try_from(event: Event) -> Result<Self, Self::Error> {
        match event {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => match (code, modifiers) {
                (KeyCode::Char('4'), KeyModifiers::SHIFT) => Ok(Self::Move(Direction::End)), //represents $
                (KeyCode::Char('h'), _) => Ok(Self::Move(Direction::Left)),
                (KeyCode::Char('k'), _) => Ok(Self::Move(Direction::Up)),
                (KeyCode::Char('j'), _) => Ok(Self::Move(Direction::Down)),
                (KeyCode::Char('l'), _) => Ok(Self::Move(Direction::Right)),
                (KeyCode::Char('0'), _) => Ok(Self::Move(Direction::Home)),
                (KeyCode::Esc, _) => Ok(Self::Exit),
                _ => Ok(Self::NoAction),
            },
            #[allow(clippy::as_conversions)]
            Event::Resize(width_u16, height_u16) => Ok(Self::Resize(Size {
                height: height_u16 as usize,
                width: width_u16 as usize,
            })),
            _ => Ok(Self::NoAction),
        }
    }
}

pub enum JumpCommand {
    Enter(usize),
    Delete,
    Move,
    Exit,
    NoAction,
}

impl TryFrom<Event> for JumpCommand {
    type Error = String;
    fn try_from(event: Event) -> Result<Self, Self::Error> {
        match event {
            Event::Key(KeyEvent { code, .. }) => match code {
                KeyCode::Char(val) => {
                    if let Some(digit) = val.to_digit(10) {
                        Ok(Self::Enter(digit.try_into().unwrap()))
                    } else {
                        Ok(Self::NoAction)
                    }
                }
                KeyCode::Backspace => Ok(Self::Delete),
                KeyCode::Esc => Ok(Self::Exit),
                KeyCode::Enter => Ok(Self::Move),
                _ => Ok(Self::NoAction),
            },
            _ => Ok(Self::NoAction),
        }
    }
}

pub enum HelpCommand {
    Exit,
    NoAction,
    Resize(Size),
}

impl TryFrom<Event> for HelpCommand {
    type Error = String;
    fn try_from(event: Event) -> Result<Self, Self::Error> {
        match event {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => match (code, modifiers) {
                (KeyCode::Char('h'), KeyModifiers::CONTROL) | (KeyCode::Esc, _) => Ok(Self::Exit),
                _ => Ok(Self::NoAction),
            },
            #[allow(clippy::as_conversions)]
            Event::Resize(width_u16, height_u16) => Ok(Self::Resize(Size {
                height: height_u16 as usize,
                width: width_u16 as usize,
            })),
            _ => Ok(Self::NoAction),
        }
    }
}
