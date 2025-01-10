use ctrlc;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

const SERVER_ADDR: &str = "127.0.0.1:8080";
const DATABASE_ADDR: &str = "127.0.0.1:2026";

// Client struct to track clients with an ID and a stream
#[derive(Clone)]
struct Client {
    id: usize,
    stream: Arc<Mutex<TcpStream>>,
}

// Database request struct that will be serialized into JSON
#[derive(Deserialize, Serialize)]
struct DatabaseRequest {
    command: String,
    data: Option<String>,
}

// Database reply struct to handle the deserialized response from the database
#[derive(Deserialize, Serialize)]
struct DatabaseReply {
    status: String,
    data: Option<String>,
}

// Function that handles timeouts for TCP connections
fn set_stream_timeout(stream: &std::net::TcpStream, duration: Duration) {
    stream
        .set_read_timeout(Some(duration))
        .expect("Failed to set read timeout");
    stream
        .set_write_timeout(Some(duration))
        .expect("Failed to set write timeout");
}

// Function to handle client communication
fn handle_client(
    stream: Arc<Mutex<TcpStream>>,
    client_id: usize,
    clients: Arc<Mutex<HashMap<usize, Client>>>,
    running: Arc<AtomicBool>,
) {
    println!("Client {} connected", client_id);

    let mut reader = BufReader::new(stream.lock().unwrap().try_clone().unwrap());
    let mut buffer = String::new();

    while running.load(Ordering::SeqCst) {
        buffer.clear();
        match reader.read_line(&mut buffer) {
            Ok(0) => {
                println!("Client {} disconnected", client_id);
                clients.lock().unwrap().remove(&client_id);
                break;
            }
            Ok(_) => {
                let trimmed_request = buffer.trim();
                let request: Result<DatabaseRequest, _> = serde_json::from_str(trimmed_request);

                match request {
                    Ok(request) => {
                        let response = match query_database(DATABASE_ADDR, &request) {
                            Ok(response) => serde_json::to_string(&response)
                                .unwrap_or_else(|_| "{\"status\":\"error\",\"data\":\"Serialization error\"}".to_string()),
                            Err(e) => format!("{{\"status\":\"error\",\"data\":\"{}\"}}", e),
                        };

                        // Send the JSON response back to the client
                        let _ = stream.lock().unwrap().write_all(format!("{}\n", response).as_bytes());
                    }
                    Err(_) => {
                        let error_response = "{\"status\":\"error\",\"data\":\"Invalid JSON format\"}\n";
                        let _ = stream.lock().unwrap().write_all(error_response.as_bytes());
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading from client {}: {}", client_id, e);
                break;
            }
        }
    }

    println!("Shutting down thread for client {}", client_id);
}

// Function to query the database
fn query_database(database_addr: &str, request: &DatabaseRequest) -> Result<DatabaseReply, String> {
    // Serialize Request Data Structure
    let request_json = serde_json::to_string(request)
        .map_err(|e| format!("Failed to serialize request: {}", e))?;

    // Connect to Centralized database
    let mut stream = TcpStream::connect(database_addr)
        .map_err(|e| format!("Failed to connect to database: {}", e))?;

    // Send JSON request to Database
    stream.write_all(format!("{}\n", request_json).as_bytes())
        .map_err(|e| format!("Failed to send request to database: {}", e))?;

    // Decode response from Database
    let mut reader = BufReader::new(&mut stream);
    let mut response_json = String::new();
    reader
        .read_line(&mut response_json)
        .map_err(|e| format!("Failed to read response: {}", e))?;

    // Deserialize the JSON response
    let response: DatabaseReply = serde_json::from_str(&response_json)
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    // Disconnect from the Database
    stream.shutdown(std::net::Shutdown::Both)
        .map_err(|e| format!("Failed to close connection with the database: {}", e))?;

    Ok(response)
}

// Main server function
fn main() {
    let listener = TcpListener::bind(SERVER_ADDR).expect("Failed to bind address");
    listener
        .set_nonblocking(true)
        .expect("Cannot set non-blocking mode");
    println!("Server listening on {}", SERVER_ADDR);

    let clients: Arc<Mutex<HashMap<usize, Client>>> = Arc::new(Mutex::new(HashMap::new()));
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = Arc::clone(&running);

    // Handle Ctrl+C for graceful shutdown
    ctrlc::set_handler(move || {
        println!("\nShutting down server...");
        running_clone.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl+C handler");

    let mut client_id_counter = 0;

    while running.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, addr)) => {
                // Log new client connection
                println!(
                    "New client connected: {} with ID {}",
                    addr, client_id_counter
                );

                set_stream_timeout(&stream, Duration::from_secs(30));
                let stream = Arc::new(Mutex::new(stream));

                let client_id = client_id_counter;
                client_id_counter += 1;

                let clients = Arc::clone(&clients);
                let running = Arc::clone(&running);

                clients.lock().unwrap().insert(
                    client_id,
                    Client {
                        id: client_id,
                        stream: Arc::clone(&stream),
                    },
                );

                // Spawn a thread to handle the client
                thread::spawn(move || handle_client(stream, client_id, clients, running));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                eprintln!("Error accepting connection: {}", e);
                break;
            }
        }
    }

    // Cleanup before exiting
    println!("Closing all client connections...");
    let clients = clients.lock().unwrap();
    for (id, client) in clients.iter() {
        println!("Closing connection for client {}", id);
        let _ = client
            .stream
            .lock()
            .unwrap()
            .shutdown(std::net::Shutdown::Both);
    }

    println!("Server terminated successfully");
}
