/****************
    IMPORTS
****************/
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
/*****************
    CONSTANTS
*****************/
const SERVER_ADDR: &str = "127.0.0.1:8080";
const DATABASE_ADDR: &str = "127.0.0.1:3036";

#[derive(Deserialize)]
struct CommandWrapper {
    command: String,
}

#[derive(Clone)]
struct Client {
    id: usize,
    stream: Arc<Mutex<TcpStream>>,
    state: CheckpointState,
}

#[derive(Deserialize, Serialize, Clone)]
enum CheckpointState {
    WaitForRfid,
    WaitForFingerprint,
    AuthSuccessful,
    AuthFailed,
}

//Authentication response struct

#[derive(Deserialize, Serialize, Clone)]
struct AuthResponse {
    status: CheckpointState,
}

#[derive(Deserialize, Serialize, Clone)]
enum EnrollUpdateDeleteStatus {
    Success,
    Failed,
}

#[derive(Deserialize, Serialize, Clone)]
struct EnrollUpdateDeleteResponse {
    status: EnrollUpdateDeleteStatus,
}

// Format for requests to the Database

#[derive(Deserialize, Serialize, Clone)]
struct DatabaseRequest {
    command: String,
    checkpoint_id: Option<u32>,
    worker_id: Option<u32>,
    worker_name: Option<String>,
    worker_fingerprint: Option<String>,
    location: Option<String>,
    authorized_roles: Option<String>,
    role_id: Option<u32>,
}

// Database response format

#[derive(Deserialize, Serialize, Clone)]
struct DatabaseReply {
    status: String,
    checkpoint_id: Option<u32>,
    worker_id: Option<u32>,
    fingerprint: Option<String>,
    data: Option<String>,
    role_id: Option<String>,
}

/*
 * Name: set_stream_timeout
 * Function: Avoid a tcp connection hanging by setting timeouts for r/w
*/

fn set_stream_timeout(stream: &std::net::TcpStream, duration: Duration) {
    stream
        .set_read_timeout(Some(duration))
        .expect("Failed to set read timeout");
    stream
        .set_write_timeout(Some(duration))
        .expect("Failed to set write timeout");
}

/*
 * Name: authenticate_rfid
 * Function: Validates RFID through DB Check. Steps:
 * 1. Create DatabaseRequest
 * 2. Compare received ID
 * 3. Return True or False
*/

fn authenticate_rfid(rfid_tag: &Option<u32>) -> bool {
    if let Some(rfid) = rfid_tag {
        let request = DatabaseRequest {
            command: "AUTHENTICATE".to_string(),
            checkpoint_id: None,
            worker_id: None,
            worker_fingerprint: None,
            location: None,
            authorized_roles: None,
            worker_name: None,
            role_id: None,
        };

        match query_database(DATABASE_ADDR, &request) {
            Ok(response) => {
                return Some(rfid) == response.worker_id.as_ref();
            }
            Err(e) => {
                eprintln!("Error querying database for RFID: {}", e);
                return false;
            }
        }
    } else {
        return false;
    }
}

/*
 * Name: authenticate_fingerprint
 * Function: Similar to rfid with logic
*/
fn authenticate_fingerprint(rfid_tag: &Option<u32>, fingerprint_hash: &Option<String>) -> bool {
    if let (Some(rfid), Some(fingerprint)) = (rfid_tag, fingerprint_hash) {
        let request = DatabaseRequest {
            command: "AUTHENTICATE".to_string(),
            checkpoint_id: None,
            worker_id: Some(rfid.clone()),
            worker_fingerprint: Some(fingerprint.clone()),
            location: None,
            authorized_roles: None,
            worker_name: None,
            role_id: None,
        };

        match query_database(DATABASE_ADDR, &request) {
            Ok(response) => {
                return Some(rfid) == response.worker_id.as_ref()
                    && Some(fingerprint) == response.fingerprint.as_ref();
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

/*
 * Name: query_database
 * Function: Establish connection and Manipulate/Interact with data in database
 * Steps:
 * 1. Create DatabaseRequest with operation
 * 2. Send through TcpStream
 * 3. Receive DatabaseReply
 * 4. Decipher response
*/

fn query_database(database_addr: &str, request: &DatabaseRequest) -> Result<DatabaseReply, String> {
    let request_json = serde_json::to_string(request)
        .map_err(|e| format!("Failed to serialize request: {}", e))?;

    let mut stream = TcpStream::connect(database_addr)
        .map_err(|e| format!("Failed to connect to database: {}", e))?;

    stream
        .write_all(
            format!(
                "{}
",
                request_json
            )
            .as_bytes(),
        )
        .map_err(|e| format!("Failed to send request to database: {}", e))?;

    let mut reader = BufReader::new(&mut stream);
    let mut response_json = String::new();
    reader
        .read_line(&mut response_json)
        .map_err(|e| format!("Failed to read response: {}", e))?;

    response_json.pop();

    let response: DatabaseReply = serde_json::from_str(&response_json)
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    stream
        .shutdown(std::net::Shutdown::Both)
        .map_err(|e| format!("Failed to close connection with the database: {}", e))?;

    Ok(response)
}

// TODO: for each functionality, call the synchronization function with the central
// database, still needs to be developed.

/*
 * Name: handle_client
 * Function: Handles communication with client
 * Steps:
 * 1. Read data using BufReader
 * 2. Parse DatabaseRequest
 * 3. Handles according to 'command' (Refer to state machine)
 * 4. Respond with result to client, update CheckpointState
*/

fn handle_client(
    stream: Arc<Mutex<TcpStream>>,
    client_id: usize,
    clients: Arc<Mutex<HashMap<usize, Client>>>,
    running: Arc<AtomicBool>,
) {
    println!("Client {} connected", client_id);

    let mut reader = BufReader::new(stream.lock().unwrap().try_clone().unwrap());
    let mut buffer = Vec::new();

    while running.load(Ordering::SeqCst) {
        buffer.clear();
        match reader.read_until(b'\0', &mut buffer) {
            Ok(0) => {
                println!("Client {} disconnected", client_id);
                clients.lock().unwrap().remove(&client_id);
                break;
            }
            Ok(_) => {
                let buffer_str = String::from_utf8(buffer.clone())
                    .expect("Failed to convert buffer to a string format");
                let trimmed_request = buffer_str.trim_end_matches('\0').trim();
                let mut request: Result<DatabaseRequest, _> = serde_json::from_str(trimmed_request);

                let mut request = request.unwrap(); // Take ownership once

                match request.command.as_str() {
                    "INIT_REQUEST" => {
                        let checkpoint_reply = match query_database(DATABASE_ADDR, &request) {
                            Ok(db_reply) => {
                                if db_reply.status == "success" {
                                    println!("Received confirmation from the database that the checkpoint was added");
                                    DatabaseReply {
                                        status: "success".to_string(),
                                        checkpoint_id: db_reply.checkpoint_id,
                                        worker_id: None,
                                        fingerprint: None,
                                        data: None,
                                        role_id: None,
                                    }
                                } else {
                                    println!("Received from the database that the checkpoint was not added");
                                    DatabaseReply {
                                        status: "error".to_string(),
                                        checkpoint_id: None,
                                        worker_id: None,
                                        fingerprint: None,
                                        data: None,
                                        role_id: None,
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Error with querying the central database: {}", e);
                                break;
                            }
                        };
                        // Send response back to client
                        let mut response_str = serde_json::to_string(&checkpoint_reply).unwrap();
                        response_str.push('\0');
                        let _ = stream
                            .lock()
                            .unwrap()
                            .write_all(format!("{}\n", response_str).as_bytes());
                    }
                    // Handle authentication logic using a state machine
                    "AUTHENTICATE" => {
                        match clients.lock().unwrap().get_mut(&client_id) {
                            Some(client) => {
                                let response = match client.state {
                                    CheckpointState::WaitForRfid => {
                                        if authenticate_rfid(&request.worker_id) {
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
                                        if authenticate_fingerprint(
                                            &request.worker_id,
                                            &request.worker_fingerprint,
                                        ) {
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
                                let _ = stream
                                    .lock()
                                    .unwrap()
                                    .write_all(format!("{}\n", response_str).as_bytes());
                            }
                            None => {
                                eprintln!("Error when getting a client");
                                break;
                            }
                        }
                    }

                    // For these functionalities, a query to the central database
                    // is performed, and the port server simply sends its response
                    // back to the checkpoint.
                    "ENROLL" => {
                        let checkpoint_reply = match query_database(DATABASE_ADDR, &request) {
                            Ok(db_reply) => {
                                if db_reply.status == "success" {
                                    EnrollUpdateDeleteResponse {
                                        status: EnrollUpdateDeleteStatus::Success,
                                    }
                                } else {
                                    EnrollUpdateDeleteResponse {
                                        status: EnrollUpdateDeleteStatus::Failed,
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to query database: {}", e);
                                EnrollUpdateDeleteResponse {
                                    status: EnrollUpdateDeleteStatus::Failed,
                                }
                            }
                        };
                        // Send response back to client
                        let response_str = serde_json::to_string(&checkpoint_reply).unwrap();
                        let _ = stream
                            .lock()
                            .unwrap()
                            .write_all(format!("{}\n", response_str).as_bytes());
                    }

                    "UPDATE" => {
                        let checkpoint_reply = match query_database(DATABASE_ADDR, &request) {
                            Ok(db_reply) => {
                                if db_reply.status == "success" {
                                    EnrollUpdateDeleteResponse {
                                        status: EnrollUpdateDeleteStatus::Success,
                                    }
                                } else {
                                    EnrollUpdateDeleteResponse {
                                        status: EnrollUpdateDeleteStatus::Failed,
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to query database: {}", e);
                                EnrollUpdateDeleteResponse {
                                    status: EnrollUpdateDeleteStatus::Failed,
                                }
                            }
                        };
                        // Send response back to client
                        let mut response_str = serde_json::to_string(&checkpoint_reply).unwrap();
                        response_str.push('\0');
                        let _ = stream
                            .lock()
                            .unwrap()
                            .write_all(format!("{}\n", response_str).as_bytes());
                    }

                    "DELETE" => {
                        let checkpoint_reply = match query_database(DATABASE_ADDR, &request) {
                            Ok(db_reply) => {
                                if db_reply.status == "success" {
                                    EnrollUpdateDeleteResponse {
                                        status: EnrollUpdateDeleteStatus::Success,
                                    }
                                } else {
                                    EnrollUpdateDeleteResponse {
                                        status: EnrollUpdateDeleteStatus::Failed,
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to query database: {}", e);
                                EnrollUpdateDeleteResponse {
                                    status: EnrollUpdateDeleteStatus::Failed,
                                }
                            }
                        };
                        // Send response back to client
                        let response_str = serde_json::to_string(&checkpoint_reply).unwrap();
                        let _ = stream
                            .lock()
                            .unwrap()
                            .write_all(format!("{}\n", response_str).as_bytes());
                    }

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
