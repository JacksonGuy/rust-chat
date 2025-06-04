use std::thread;
use std::sync::{Arc, Mutex};
use std::io::{BufReader, BufWriter, Write};
use std::net::{TcpStream};
use serde::{Deserialize};

pub mod core;
use crate::core::{
    ui::App,
    net::{
        self,
        Packet, PacketType,
        ClientState,
    },
};

fn main() {
    // Connect to server
    let stream = TcpStream::connect("127.0.0.1:8080")
        .expect("Failed to connect to server");

    // Initialize state
    let state = Arc::new(Mutex::new(ClientState::default()));

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

    // Get username
    let mut username = String::new();
    println!("Enter a username");
    print!("> ");
    let _ = std::io::stdout().flush();
    let _ = std::io::stdin().read_line(&mut username).unwrap();
    
    // Send username to server
    let username_packet = Packet {
        packet_type: PacketType::UsernameChange,
        user_id: uid,
        contents: username.clone(),
    };
    let json = serde_json::to_string(&username_packet)
        .expect("[ERROR] Failed to serialize packet.");
    writer.write(json.as_bytes()).expect("[ERROR] Failed to write username");
    writer.flush().expect("[ERROR] Failed to send username.");

    // Add self to state
    {
        let mut s = state.lock().unwrap();
        s.users.insert(uid, username.clone());
        s.username = username;
    }

    // Create UI
    let app = App::new(writer, uid);
    let terminal = ratatui::init();

    // Start threads
    let state_clone = state.clone();
    let listen_thread = thread::spawn(move || net::server_listen(reader, state_clone));
    let ui_thread = thread::spawn(move || app.run(terminal, state.clone()));
    
    listen_thread.join().unwrap();
    let _ = ui_thread.join().unwrap();
}
