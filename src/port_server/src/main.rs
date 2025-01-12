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
const DATABASE_ADDR: &str = "127.0.0.1:3036";

// Client struct to track clients with an ID and a stream
#[derive(Clone)]
struct Client {
    id: usize,
    stream: Arc<Mutex<TcpStream>>,
    state: CheckpointState,
}

// Messages to send back to a checkpoint
#[derive(Deserialize, Serialize, Clone)]
enum CheckpointState {
    WaitForRfid,
    WaitForFingerprint,
    AuthSuccessful,
    AuthFailed,
}

#[derive(Deserialize, Serialize, Clone)]
struct AuthResponse {
    status: CheckpointState,
}

// Database request struct that will be serialized into JSON
#[derive(Deserialize, Serialize, Clone)]
struct DatabaseRequest {
    command: String,
    rfid: Option<String>,
    fingerprint: Option<String>,
    data: Option<String>,
}

// Database reply struct to handle the deserialized response from the database
#[derive(Deserialize, Serialize, Clone)]
struct DatabaseReply {
    status: String,
    rfid: Option<String>,
    fingerprint: Option<String>,
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

//TODO: We need a function to check is a user is in the port server so authentication
// can be done locally

//TODO: We need a function to add a user to the port server's hash map and keep it
// synchronized with the central database


// Perform RFID authentication
fn authenticate_rfid(rfid_tag: &Option<String>) -> bool {
    if let Some(rfid) = rfid_tag {

        // TODO: implement a function to see if a user is in the server's hash table
        // else query the main database and add to the existing hash table.
        // It should be called and checked before doing a query.

        let request = DatabaseRequest {
            command: "AUTHENTICATE".to_string(),
            rfid: Some(rfid.clone()),
            fingerprint: None,
            data: None,
        };

        // Ensure the RFID given is in the database
        match query_database(DATABASE_ADDR, &request) {
            Ok(response) => {
                return Some(rfid) == response.rfid.as_ref();
            }
            Err(e) => {
                eprintln!("Error querying database for RFID: {}", e);
                return false;
            }
        }
    } else {
        return false
    }
}

// Perform fingerprint authentication
fn authenticate_fingerprint(rfid_tag: &Option<String>,
    fingerprint_hash: &Option<String>) -> bool {
    if let (Some(rfid), Some(fingerprint)) = (rfid_tag, fingerprint_hash) {
        // TODO: see authenticate_rfid
        let request = DatabaseRequest {
            command: "AUTHENTICATE".to_string(),
            rfid: Some(rfid.clone()),
            fingerprint: Some(fingerprint.clone()),
            data: None,
        };

        match query_database(DATABASE_ADDR, &request) {
            Ok(response) => {
                return Some(rfid) == response.rfid.as_ref() &&
                    Some(fingerprint) == response.fingerprint.as_ref();
            }
            Err(e) => {
                eprintln!("Error querying database for fingerprint hash: {}", e);
                return false;
            }
        }
    } else {
        return false;
    }
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

                let request = request.unwrap(); // Take ownership once

                match request.command.as_str() {
                    // Handle authentication logic using a state machine
                    "AUTHENTICATE" => {
                        match clients.lock().unwrap().get_mut(&client_id) {
                            Some(client) => {
                                let response = match client.state {
                                    CheckpointState::WaitForRfid => {
                                        if authenticate_rfid(&request.rfid) {
                                            client.state = CheckpointState::WaitForFingerprint;
                                            AuthResponse {
                                                status: CheckpointState::WaitForFingerprint,
                                            }
                                        } else {
                                            client.state = CheckpointState::AuthFailed;
                                            AuthResponse {
                                                status: CheckpointState::AuthFailed,
                                            }
                                        }
                                    }
                                    CheckpointState::WaitForFingerprint => {
                                        if authenticate_fingerprint(&request.rfid, &request.fingerprint) {
                                            client.state = CheckpointState::AuthSuccessful;
                                            AuthResponse {
                                                status: CheckpointState::AuthSuccessful,
                                            }
                                        } else {
                                            client.state = CheckpointState::AuthFailed;
                                            AuthResponse {
                                                status: CheckpointState::AuthFailed,
                                            }
                                        }
                                    }
                                    CheckpointState::AuthSuccessful => {
                                        thread::sleep(Duration::from_secs(5));
                                        client.state = CheckpointState::WaitForRfid;
                                        AuthResponse {
                                            status: CheckpointState::WaitForRfid,
                                        }
                                    }
                                    CheckpointState::AuthFailed => {
                                        thread::sleep(Duration::from_secs(5));
                                        client.state = CheckpointState::WaitForRfid;
                                        AuthResponse {
                                            status: CheckpointState::WaitForRfid,
                                        }
                                    }
                                };
                                // Send response back to client
                                let response_str = serde_json::to_string(&response).unwrap();
                                let _ = stream.lock().unwrap().write_all(format!("{}\n", response_str).as_bytes());
                            }
                            None => {
                                eprintln!("Error when getting a client");
                                break;
                            }
                        }
                    }

                    //TODO: Handle other functionalities on the server side, 
                    // (Enroll, Update, Delete)
    
                    _ => {
                        eprintln!("Unknown command");
                        break;
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
                        state: CheckpointState::WaitForRfid,
                    },
                );

                // Spawn a thread to handle the client
                thread::spawn(move || handle_client(stream, client_id, clients, running));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
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
