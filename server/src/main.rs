use std::collections::HashMap;
use core::fmt;
use std::sync::{Arc};
use serde::{Serialize, Deserialize};
use tokio::{
    io::{AsyncWriteExt, AsyncReadExt, BufReader, BufWriter},
    net::{TcpStream, TcpListener},
    sync::{
        Mutex,
        broadcast::{self, Sender},
    },
};

#[derive(Default, Clone, Serialize, Deserialize)]
struct User {
    uid: u32,
    name: String,
    messages: Vec<u32>,
}

#[derive(Default, Clone, Serialize, Deserialize)]
struct Message {
    uid: u32,
    sender_id: u32,
    message: String,
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.sender_id, self.message)
    }
}

#[derive(Default)]
struct ServerState {
    user_list: HashMap<u32, User>,
    message_list: Vec<Message>,
}

#[derive(Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum PacketType {
    #[default]
    None,
    IDAssign,
    UserConnected,
    UserDisconnected,
    UserList,
    UsernameChange,
    NewMessage,
}

#[derive(Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Packet {
    packet_type: PacketType, 
    
    user_id: u32,
    contents: String,
} 

async fn handle_client(
    mut tcp_stream: TcpStream,
    sender: Sender<Packet>,
    state: Arc<Mutex<ServerState>>,
) -> std::io::Result<()> {
    // Subscribe to broadcast channel
    let mut receiver = sender.subscribe();

    // Split TCP Stream
    let (read, write) = tcp_stream.split();
    let mut reader = BufReader::new(read);
    let mut writer = BufWriter::new(write);

    // Send UID to client
    let uid: u32 = rand::random::<u32>();
    let packet: Packet = Packet {
        packet_type: PacketType::IDAssign,
        user_id: uid,
        ..Default::default()
    };
    let data = serde_json::to_string(&packet)
        .expect("[ERROR] Failed to serialize packet");
    writer.write(data.as_bytes()).await?;
    writer.flush().await?;

    // Get username from client
    let mut buffer = [0; 1024];
    let mut packet = loop {
        let _ = reader.read(&mut buffer).await;
        let mut data = serde_json::Deserializer::from_slice(&buffer);
        let packet: Packet = Packet::deserialize(&mut data)
            .expect("[ERROR] Failed to deserialize packet");

        if packet.packet_type == PacketType::UsernameChange {
            break packet;
        }
    };

    // Create user object for new client
    packet.contents = packet.contents.trim().to_string();
    println!("[SERVER] New User: {}", packet.contents.clone());
    let mut local: User = User {
        uid: uid,
        name: packet.contents,
        ..Default::default()
    };
    
    // Add user to state
    {
        let mut s = state.lock().await;
        s.user_list.insert(local.uid, local.clone());

        // Broadcast new user packet
        let new_user_packet = Packet {
            packet_type: PacketType::UserConnected,
            user_id: local.uid,
            contents: local.name.clone(),
        };
        let _ = sender.send(new_user_packet);

        // Send client list of users
        for (_, user) in &s.user_list {
            // Don't send the local user a copy of themself
            if user.uid == local.uid {
                continue;
            }

            let user_list_packet = Packet {
                packet_type: PacketType::UserList,
                user_id: user.uid,
                contents: user.name.clone(),
            };
            let user_data = serde_json::to_string(&user_list_packet)
                .expect("[ERROR] Failed to serialize packet");
            writer.write(user_data.as_bytes()).await?;
            writer.flush().await?;
        }
    }

    // Main client handle loop
    loop {
        let mut buffer = [0; 1024];
        
        // This allows us to process multiple different "types" of
        // messages from the client. 
        tokio::select! {
            // Process data read from the client
            socket_read_result = reader.read(&mut buffer) => {
                let num_bytes: usize = socket_read_result?;

                if num_bytes == 0 {
                    break;
                }

                // Convert recieved data into packet object
                //let packet: Packet = serde_json::from_str(&buffer).unwrap();
                let mut data = serde_json::Deserializer::from_slice(&buffer);    
                let packet = Packet::deserialize(&mut data)
                    .expect("[ERROR] Failed to deserialize packet");

                // Handle Packet
                let packet_clone = packet.clone();
                match packet.packet_type {
                    PacketType::UsernameChange => {
                        local.name = packet.contents.clone();
                        {
                            let mut s = state.lock().await;
                            let user = s.user_list.get_mut(&local.uid).unwrap();
                            user.name = packet.contents.clone();
                        }
                    },
                    PacketType::NewMessage => {
                        let message = Message {
                            uid: rand::random::<u32>(),    
                            sender_id: local.uid,
                            message: packet.contents.trim().to_string(),    
                        };
                        {
                            let mut s = state.lock().await;
                            s.message_list.push(message.clone());
                        }
                    },
                    _ => {
                        println!("[SERVER] Unknown packet received");
                    },
                }

                // Redirect packet to broadcast channel
                let _ = sender.send(packet_clone);
            }

            // Send data from broadcast channel to client
            channel_read_result = receiver.recv() => {
                if let Ok(packet) = channel_read_result {
                    if packet.user_id != local.uid ||
                        packet.packet_type == PacketType::NewMessage ||
                        packet.packet_type == PacketType::UsernameChange
                    {
                        let data = serde_json::to_string(&packet).unwrap();
                        let _ = writer.write(data.as_bytes()).await?;
                        writer.flush().await?;
                    }
                } 
            }
        }
    }

    // Remove user from list
    let mut s = state.lock().await;
    s.user_list.remove(&local.uid);

    // Broadcast Disconnect Packet
    let packet = Packet {
        packet_type: PacketType::UserDisconnected,
        user_id: local.uid,
        contents: String::new(),
    };
    let _ = sender.send(packet);
    
    Ok(())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let state: Arc<Mutex<ServerState>> = Arc::new(Mutex::new(ServerState::default()));

    // Create listener
    let listener = TcpListener::bind("127.0.0.1:8080")
        .await
        .expect("Error: Failed to bind to port");
    println!("Server listening on port 8080");

    // Create broadcast channel
    let (channel, _) = broadcast::channel::<Packet>(10);

    // Server Loop. Listen for new connections
    loop {
        // Accept connection
        let (client_stream, _) = listener.accept().await?;
        println!("[SERVER] Connected Received");

        // Create task to handle connection
        let channel_clone = channel.clone();
        let state_clone = state.clone();
        tokio::spawn(async move {
            match handle_client(client_stream, channel_clone, state_clone).await {
                Ok(_) => println!("[SERVER] Client Disconnected"),
                Err(error) => println!("[ERROR] Failed to handle connection: {}", error)
            };
        });
    }
}
