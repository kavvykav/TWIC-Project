use std::collections::HashMap;
use std::io::{Read};  // Removed Write import
use std::net::{TcpListener, TcpStream, SocketAddr};
use std::sync::{Arc, Mutex};
use std::thread;

const SERVER_ADDR: &str = "127.0.0.1:7878";

// Struct to represent each connected client
#[derive(Clone)]
struct Client {
    id: usize,
    stream: Arc<Mutex<TcpStream>>, // Stream is part of the struct
}

fn handle_client(
    client: Client, // Accept the whole client struct
    clients: Arc<Mutex<HashMap<SocketAddr, Client>>>,
) {
    let addr = client.stream.lock().unwrap().peer_addr().unwrap();  // Access stream and its peer_addr
    println!("Client {} connected from {}", client.id, addr);

    let mut buffer = [0; 512];

    loop {
        match client.stream.lock().unwrap().read(&mut buffer) {
            Ok(0) => {
                println!("Client {} disconnected", client.id);
                break;
            }
            Ok(n) => {
                let message = String::from_utf8_lossy(&buffer[..n]);
                println!("Received from client {}: {}", client.id, message);

                // Here you could process the message or forward it to another client
            }
            Err(e) => {
                eprintln!("Failed to read from client {}: {}", client.id, e);
                break;
            }
        }
    }

    // Remove client from active list on disconnect
    let mut clients = clients.lock().unwrap();
    clients.remove(&addr); // Remove based on client address
}

fn main() {
    let listener = TcpListener::bind(SERVER_ADDR).expect("Failed to bind address");
    println!("Server listening on {}", SERVER_ADDR);

    let clients: Arc<Mutex<HashMap<SocketAddr, Client>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut client_id_counter = 0;

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let addr = stream.peer_addr().unwrap();
                let clients_clone = Arc::clone(&clients);
                let mut clients = clients.lock().unwrap();

                // Check if the client is already connected (by IP:port)
                let _client_id = if let Some(existing_client) = clients.get(&addr) {
                    existing_client.clone().id  // Clone the client and access its id
                } else {
                    // If the client is new, assign a new ID
                    client_id_counter += 1;
                    clients.insert(
                        addr,
                        Client {
                            id: client_id_counter,
                            stream: Arc::new(Mutex::new(stream.try_clone().unwrap())),
                        },
                    );
                    client_id_counter
                };

                let client = clients.get(&addr).unwrap().clone(); // Get the client from the map
               

                // Handle each client in a new thread
                thread::spawn(move || handle_client(client, clients_clone));
            }
            Err(e) => eprintln!("Failed to accept connection: {}", e),
        }
    }
}
