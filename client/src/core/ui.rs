use std::process;
use std::time::Duration;
use std::io::{self, BufWriter, Write};
use std::net::{TcpStream};
use std::sync::{Arc, Mutex};
use ratatui::{
    crossterm::event::{self, Event, KeyCode},
    layout::{Constraint, Layout,},
    style::{Style, Stylize},
    text::{Line,},
    widgets::{Block, List, Paragraph, ListItem},
    DefaultTerminal, Frame,
};

use crate::net::{ClientState, Packet, PacketType};

pub struct App {
    input: String,
    character_index: usize,
    stream: BufWriter<TcpStream>,
    user_id: u32,
}

impl App {
    pub fn new(stream: BufWriter<TcpStream>, uid: u32) -> Self {
        Self {
            input: String::new(),
            character_index: 0,
            stream: stream,
            user_id: uid,
        }
    }

    fn clamp_cursor(&self, pos: usize) -> usize {
        pos.clamp(0, self.input.chars().count())
    }

    fn move_cursor_left(&mut self) {
        let pos = self.character_index.saturating_sub(1);
        self.character_index = self.clamp_cursor(pos);
    }

    fn move_cursor_right(&mut self) {
        let pos = self.character_index.saturating_add(1);
        self.character_index = self.clamp_cursor(pos);
    }

    // Get the current input size in bytes
    fn byte_index(&self) -> usize {
        self.input.chars().count()
    }

    fn enter_char(&mut self, c: char) {
        let index = self.byte_index();
        self.input.insert(index, c);
        self.move_cursor_right();
    }

    fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.character_index != 0;
        if is_not_cursor_leftmost {
            let before_cursor = self.input.chars().take(self.character_index - 1);
            let after_cursor = self.input.chars().skip(self.character_index);

            self.input = before_cursor.chain(after_cursor).collect();
            self.move_cursor_left();
        }
    }

    fn submit_message(&mut self) {
        let start = match self.input.chars().nth(0) {
            Some(c) => c,
            None => '!'
        };

        match start {
            '/' => {
                let packet = self.parse_command(self.input.clone());
                match packet {
                    None => (),
                    Some(packet) => {
                        let data = serde_json::to_string(&packet)
                            .expect("[ERROR] Failed to serialize packet");
                        let _ = self.stream.write(data.as_bytes());
                        self.stream.flush().expect("[ERROR] Failed to send message");
                    }
                }
            },
            '!' => (),
            _ => {
                let packet = Packet {
                    packet_type: PacketType::NewMessage,
                    user_id: self.user_id,
                    contents: self.input.clone(),
                };
                let data = serde_json::to_string(&packet)
                    .expect("[ERROR] Failed to serialize packet");
                let _ = self.stream.write(data.as_bytes());
                self.stream.flush().expect("[ERROR] Failed to send message");
            }
        }

        self.input.clear();
        self.character_index = 0;
    }

    fn parse_command(&mut self, command: String) -> Option<Packet> {
        let tokens: Vec<&str> = command.split_whitespace().collect();
        
        if tokens.len() < 2 {
            return None
        }

        let first = tokens[0];

        match first {
            "/name" => {
                Some(Packet {
                    packet_type: PacketType::UsernameChange,
                    user_id: self.user_id,
                    contents: tokens[1].to_string(),
                })
            },
            _ => None
        }
    }

    // Run this as a separate thread
    pub fn run(
        mut self, 
        mut terminal: DefaultTerminal, 
        state: Arc<Mutex<ClientState>>,
    ) -> io::Result<()> {
        loop {
            terminal.draw(|frame| self.draw(frame, &state))?;
            
            if event::poll(Duration::from_millis(16))? { 
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Esc => {
                            ratatui::restore();
                            process::exit(1);
                        },
                        KeyCode::Enter => self.submit_message(),
                        KeyCode::Char(to_insert) => self.enter_char(to_insert),
                        KeyCode::Backspace => self.delete_char(),
                        KeyCode::Left => self.move_cursor_left(),
                        KeyCode::Right => self.move_cursor_right(),
                        _ => (),
                    }
                }
            }
        }
    }

    fn draw(&self, frame: &mut Frame, state: &Arc<Mutex<ClientState>>) {
        let vertical = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(3),
        ]);
        let horizontal = Layout::horizontal([
            Constraint::Percentage(80),
            Constraint::Percentage(20),
        ]);
        let [content, users_area] = horizontal.areas(frame.area());
        let [message_area, input_area] = vertical.areas(content);

        let s = state.lock().unwrap();

        // Render messages
        let messages: Vec<ListItem> = s.messages
            .iter()
            .enumerate()
            .map(|(_, message)| {
                let start = message.chars().nth(0).unwrap();
                let item;
                if start == '(' {
                    item = Line::from(message.clone());
                }
                else {
                    item = Line::from(message.clone()).red();
                }
                ListItem::new(item)
            })
            .collect();
        let messages = List::new(messages).block(Block::bordered().title("Messages"));
        frame.render_widget(messages, message_area);

        // Render Input Box
        let input = Paragraph::new(self.input.as_str())
            .style(Style::default())
            .block(Block::bordered().title("Input"));
        frame.render_widget(input, input_area);
        frame.set_cursor_position((
            input_area.x + self.character_index as u16 + 1,
            input_area.y + 1,
        ));

        // Render user list
        let mut users: Vec<ListItem> = vec![];
        for (_, name) in s.users.iter() {
            users.push(ListItem::new(Line::from(name.clone())));
        }
        let users = List::new(users).block(Block::bordered().title("Users"));
        frame.render_widget(users, users_area);
    }
}
