use std::collections::HashMap;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use rand::Rng;
use std::time::Duration;

const SERVER_ADDR: &str = "127.0.0.1:7878";

#[derive(Clone)]
#[allow(dead_code)] // surpress warnings for unread fields
struct Client {
    id: usize,
    stream: Arc<Mutex<TcpStream>>,
}

fn handle_client(
    stream: Arc<Mutex<TcpStream>>,
    client_id: usize,
    clients: Arc<Mutex<HashMap<usize, Client>>>,
) {
    println!("Client {} connected", client_id);
    let send_stream = Arc::clone(&stream);

    // Thread for sending random numbers to the client
    thread::spawn(move || {
        let mut rng = rand::thread_rng();
        loop {
            let random_number = rng.gen_range(1..=100);
            let message = format!("Random number: {}\n", random_number);

            // Attempt to send the random number to the client
            match send_stream.lock().unwrap().write(message.as_bytes()) {
                Ok(_) => {

                }
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::BrokenPipe || e.kind() == std::io::ErrorKind::ConnectionReset{
                        // Remove the client from the active list on disconnect
                        clients.lock().unwrap().remove(&client_id);
                        println!("Client {} removed from active list", client_id);
                    } else {
                        eprintln!("Failed to send message to the client: {}", e);
                        break;
                    }
                }
            }
            thread::sleep(Duration::from_secs(1));
        }
    });

} 

fn main() {
    let listener = TcpListener::bind(SERVER_ADDR).expect("Failed to bind address");
    println!("Server listening on {}", SERVER_ADDR);

    let target = 0;
    let clients: Arc<Mutex<HashMap<usize, Client>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut client_id_counter = 0;

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let client_id = client_id_counter;
                client_id_counter += 1;

                let stream = Arc::new(Mutex::new(stream));
                let clients = Arc::clone(&clients);

                // Register the new client
                clients.lock().unwrap().insert(
                    client_id,
                    Client {
                        id: client_id,
                        stream: Arc::clone(&stream),
                    },
                );

                // Spawn a thread to handle the client
                thread::spawn(move || handle_client(stream, target, clients));
            }
            Err(e) => eprintln!("Failed to accept connection: {}", e),
        }
    }
}

