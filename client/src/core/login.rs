use std::process;
use std::time::Duration;
use std::io::{self, BufReader, BufWriter, Write};
use std::net::{TcpStream};
use serde::{Deserialize};
use ratatui::{
    crossterm::event::{self, Event, KeyCode},
    layout::{Constraint, Layout,},
    style::{Style},
    widgets::{Block, Paragraph,},
    DefaultTerminal, Frame,
};

use crate::core::net::{Packet, PacketType};

pub struct Login {
    address_input: String,
    username_input: String,
    character_index: usize,
    input_select: u8,

    reader: Option<BufReader<TcpStream>>,
    writer: Option<BufWriter<TcpStream>>,
    uid: Option<u32>,
}

impl Login {
    pub fn new() -> Self {
        Self {
            address_input: String::new(),
            username_input: String::new(),
            character_index: 0,
            input_select: 0,
            reader: None,
            writer: None,
            uid: None,
        }
    }

    fn clamp_cursor(&self, pos: usize) -> usize {
        let length = match self.input_select {
            0 => self.address_input.chars().count(),
            1 => self.username_input.chars().count(),
            _ => 0,
        };
        pos.clamp(0, length)
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
        let string = match self.input_select {
            0 => self.address_input.clone(),
            1 => self.username_input.clone(),
            _ => String::new(),
        };

        string
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.character_index)
            .unwrap_or(string.len())
    }

    fn enter_char(&mut self, c: char) {
        let index = self.byte_index();
        match self.input_select {
            0 => self.address_input.insert(index, c),
            1 => self.username_input.insert(index, c),
            _ => ()
        }
        self.move_cursor_right();
    }

    fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.character_index != 0;
        if is_not_cursor_leftmost {
            match self.input_select {
                0 => {
                    let before_cursor = self.address_input.chars().take(self.character_index - 1);
                    let after_cursor = self.address_input.chars().skip(self.character_index);

                    self.address_input = before_cursor.chain(after_cursor).collect();
                },
                1 => {
                    let before_cursor = self.username_input.chars().take(self.character_index - 1);
                    let after_cursor = self.username_input.chars().skip(self.character_index);

                    self.username_input = before_cursor.chain(after_cursor).collect();
                },
                _ => (),
            }
            
            self.move_cursor_left();
        }
    }

    fn switch_inputs(&mut self) {
        self.input_select = (self.input_select + 1) % 2;
        self.character_index = self.byte_index();
    }

    fn submit_login(&mut self) {
        // Connect to server
        let stream = TcpStream::connect("127.0.0.1:8080")
            .expect("Failed to connect to server");

        // Split TCP Stream
        let stream_clone = stream.try_clone().unwrap();
        let mut reader = BufReader::new(stream);
        let mut writer = BufWriter::new(stream_clone);

        // Get UserID from server
        let uid = loop {
            let mut data = serde_json::Deserializer::from_reader(&mut reader);
            let packet: Packet = Packet::deserialize(&mut data)
                .expect("[ERROR] Failed to deserialize packet");

            if packet.packet_type == PacketType::IDAssign {
                break packet.user_id
            }
            else {
                println!("[ERROR] Unexpected packet type");
            }
        };

        // Send username to server
        let username_packet = Packet {
            packet_type: PacketType::UsernameChange,
            user_id: uid,
            contents: self.username_input.clone(),
        };
        let json = serde_json::to_string(&username_packet)
            .expect("[ERROR] Failed to serialize packet.");
        writer.write(json.as_bytes()).expect("[ERROR] Failed to write username");
        writer.flush().expect("[ERROR] Failed to send username.");
   
        self.uid = Some(uid);
        self.reader = Some(reader);
        self.writer = Some(writer);
    }

    pub fn get_results(self) -> (u32, String, BufReader<TcpStream>, BufWriter<TcpStream>) {
        (self.uid.unwrap(), self.username_input, self.reader.unwrap(), self.writer.unwrap())
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;

            if event::poll(Duration::from_millis(16))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Esc => {
                            ratatui::restore();
                            process::exit(0);
                        },
                        KeyCode::Enter => {
                            self.submit_login();
                            break;
                        }
                        KeyCode::Tab => self.switch_inputs(),
                        KeyCode::Char(to_insert) => self.enter_char(to_insert),
                        KeyCode::Backspace => self.delete_char(),
                        KeyCode::Left => self.move_cursor_left(),
                        KeyCode::Right => self.move_cursor_right(),
                        _ => (),
                    }
                }
            }
        }

        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        let horizontal = Layout::horizontal([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ]);
        let vertical = Layout::vertical([
            Constraint::Percentage(20),
            Constraint::Percentage(80),
        ]);
        let input_prompts = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(3),
        ]);

        let [_, middle, _] = horizontal.areas(frame.area());
        let [_, center] = vertical.areas(middle);
        let [server_input_area, username_input_area] = input_prompts.areas(center);

        // Server Address input
        let server_input = Paragraph::new(self.address_input.as_str())
            .style(Style::default())
            .block(Block::bordered().title("Server"));
        frame.render_widget(server_input, server_input_area);

        // Username input
        let name_input = Paragraph::new(self.username_input.as_str())
            .style(Style::default())
            .block(Block::bordered().title("Username"));
        frame.render_widget(name_input, username_input_area);
    
        match self.input_select {
            0 => {
                frame.set_cursor_position((
                    server_input_area.x + self.character_index as u16 + 1,
                    server_input_area.y + 1,
                ));
            },
            1 => {
                frame.set_cursor_position((
                    username_input_area.x + self.character_index as u16 + 1,
                    username_input_area.y + 1,
                ));
            }
            _ => (),
        }
    }
}
