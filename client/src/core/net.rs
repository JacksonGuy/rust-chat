use std::sync::{Arc, Mutex};
use std::collections::{HashMap};
use std::io::{BufReader};
use std::net::{TcpStream};
use serde::{Serialize, Deserialize};

#[derive(Default, Serialize, Deserialize)]
pub struct Message {
    pub uid: u32,
    pub sender_id: u32,
    pub message: String,
}

#[derive(Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PacketType {
    #[default]
    None,
    IDAssign,
    UserConnected,
    UserDisconnected,
    UserList,
    UsernameChange,
    NewMessage,
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Packet {
    pub packet_type: PacketType, 
    
    pub user_id: u32,
    pub contents: String,
} 

#[derive(Default)]
pub struct ClientState {
    pub username: String,
    pub users: HashMap<u32, String>,
    pub messages: Vec<String>,
}

pub fn server_listen(mut stream: BufReader<TcpStream>, state: Arc<Mutex<ClientState>>) {
    loop {
        let mut data = serde_json::Deserializer::from_reader(&mut stream);
        let packet = Packet::deserialize(&mut data)
            .expect("[ERROR] Failed to deserialize packet");

        let mut s = state.lock().unwrap();

        match packet.packet_type {
            PacketType::UserConnected => {
                s.users.insert(packet.user_id, packet.contents.clone());
                s.messages.push(format!("{} joined the chat", packet.contents));
            },
            PacketType::UserDisconnected => {
                let user = s.users.get(&packet.user_id)
                    .expect("[ERROR] User doesn't exist")
                    .clone();
                s.messages.push(format!("{} left the chat", user));
                s.users.remove(&packet.user_id).expect("[ERROR] Failed to remove user");
            },
            PacketType::UserList => {
                s.users.insert(packet.user_id, packet.contents.clone());
            }
            PacketType::UsernameChange => {
                let user = s.users.get_mut(&packet.user_id)
                    .expect("[ERROR] User does not exist");
                let old_name = user.clone();
                *user = packet.contents.clone();
                s.messages.push(format!("{} changed their name to {}", old_name, packet.contents.clone()));
            },
            PacketType::NewMessage => {
                let username = s.users.get(&packet.user_id)
                    .expect("[ERROR] User does not exist")
                    .clone();
                s.messages.push(format!("({}) {}", username, packet.contents.trim()));
            },
            _ => () 
        }
    }
}
