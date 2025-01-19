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


#[derive(Deserialize, Serialize, Clone)]
enum EnrollUpdateDeleteStatus {
    Success,
    Failed,
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
    worker_fingerprint: Option<String>,
    data: Option<String>,
    role_id: Option<u32>,
    auth_response: Option<CheckpointState>,
    update_delete_enroll_result: Option<EnrollUpdateDeleteStatus>,
}

impl DatabaseReply {
    pub fn success(checkpoint_id: Option<u32>) -> Self {
        DatabaseReply {
            status: "success".to_string(),
            checkpoint_id: checkpoint_id,
            worker_id: None,
            worker_fingerprint: None,
            data: None,
            role_id: None,
            auth_response: None,
            update_delete_enroll_result: None,
        }
    }

    pub fn error() -> Self {
        DatabaseReply {
            status: "error".to_string(),
            checkpoint_id: None,
            worker_id: None,
            worker_fingerprint: None,
            data: None,
            role_id: None,
            auth_response: None,
            update_delete_enroll_result: None,
        }
    }
    pub fn auth_reply(state: CheckpointState) -> Self {
        DatabaseReply {
            status: "success".to_string(),
            checkpoint_id: None,
            worker_id: None,
            worker_fingerprint: None,
            data: None,
            role_id: None,
            auth_response: Some(state),
            update_delete_enroll_result: None,
        }
    }
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

fn authenticate_rfid(rfid_tag: &Option<u32>, checkpoint_id: &Option<u32>) -> bool {
    if let (Some(rfid), Some(checkpoint)) = (rfid_tag, checkpoint_id) {
        let request = DatabaseRequest {
            command: "AUTHENTICATE".to_string(),
            checkpoint_id: Some(checkpoint.clone()),
            worker_id: Some(rfid.clone()),
            worker_fingerprint: None,
            location: None,
            authorized_roles: None,
            worker_name: None,
            role_id: None,
        };

        match query_database(DATABASE_ADDR, &request) {
            Ok(response) => {
                println!("RFID comparison: from checkpoint: {}, from database: {:?}", rfid, response.worker_id);
                println!("Response status: {}", response.status);

                // Error check
                if response.status != "success".to_string() {
                    return false;
                }
                return Some(rfid) == response.worker_id.as_ref();
            }
            Err(e) => {
                eprintln!("Error querying database for RFID: {:?}", e);
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
fn authenticate_fingerprint(rfid_tag: &Option<u32>, fingerprint_hash: &Option<String>, checkpoint_id: &Option<u32>) -> bool {
    if let (Some(rfid), Some(fingerprint), Some(checkpoint)) = (rfid_tag, fingerprint_hash, checkpoint_id) {
        let request = DatabaseRequest {
            command: "AUTHENTICATE".to_string(),
            checkpoint_id: Some(checkpoint.clone()),
            worker_id: Some(rfid.clone()),
            worker_fingerprint: Some(fingerprint.clone()),
            location: None,
            authorized_roles: None,
            worker_name: None,
            role_id: None,
        };

        match query_database(DATABASE_ADDR, &request) {
            Ok(response) => {
                println!("RFID comparison: from checkpoint: {}, from database: {:?}", rfid, response.worker_id);
                println!("Fingerprint comparison: from checkpoint: {}, from database: {:?}", fingerprint, response.worker_fingerprint);

                // Error check
                if response.status != "success".to_string() {
                    return false;
                }

                return Some(rfid) == response.worker_id.as_ref()
                    && Some(fingerprint) == response.worker_fingerprint.as_ref();
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
                "{}",
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

/*
 * Name: handle_client
 * Function: Allows a client to connect, instantiates a buffer and a reader and polls for oncoming requests.
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
        if let Err(e) = read_request(&mut reader, &stream, client_id, &clients, &mut buffer) {
            eprintln!("Error processing client {}: {}", client_id, e);
            break;
        }
    }

    println!("Shutting down thread for client {}", client_id);
    clients.lock().unwrap().remove(&client_id);
}

/*
 * Name: read_request
 * Function: Reads and deserializes an oncoming request.
 */
fn read_request(
    reader: &mut BufReader<TcpStream>,
    stream: &Arc<Mutex<TcpStream>>,
    client_id: usize,
    clients: &Arc<Mutex<HashMap<usize, Client>>>,
    buffer: &mut Vec<u8>,
) -> Result<(), String> {
    buffer.clear();
    match reader.read_until(b'\0', buffer) {
        Ok(0) => Err("Client disconnected".into()),
        Ok(_) => {
            buffer.pop();
            let request_str = parse_request(buffer)?;
            let request: DatabaseRequest = serde_json::from_str(&request_str)
                .map_err(|e| format!("Failed to parse request: {}", e))?;
            parse_command_from_request(request, stream, client_id, clients)?;
            Ok(())
        }
        Err(e) => Err(format!("Error reading from client: {}", e)),
    }
}

fn parse_request(buffer: &[u8]) -> Result<String, String> {
    String::from_utf8(buffer.to_vec())
        .map(|s| s.trim_end_matches('\0').trim().to_string())
        .map_err(|e| format!("Failed to convert buffer to string: {}", e))
}

/*
 * Name: parse_command_from_request
 * Function: Extracts the command from the request and calls the appropriate handler.
 */
fn parse_command_from_request(
    request: DatabaseRequest,
    stream: &Arc<Mutex<TcpStream>>,
    client_id: usize,
    clients: &Arc<Mutex<HashMap<usize, Client>>>,
) -> Result<(), String> {
    match request.command.as_str() {
        "INIT_REQUEST" => handle_init_request(request, stream),
        "AUTHENTICATE" => handle_authenticate(request, stream, client_id, clients),
        "ENROLL" => handle_database_request(request, stream, "ENROLL"),
        "UPDATE" => handle_database_request(request, stream, "UPDATE"),
        "DELETE" => handle_database_request(request, stream, "DELETE"),
        _ => Err("Unknown command".into()),
    }
}

/*
 * Name: handle_init_request
 * Function: Handler for a checkpoint init_request.
 */
fn handle_init_request(
    request: DatabaseRequest,
    stream: &Arc<Mutex<TcpStream>>,
) -> Result<(), String> {
    let reply = query_database(DATABASE_ADDR, &request).map(|db_reply| {
        if db_reply.status == "success" {
            DatabaseReply::success(Some(db_reply.checkpoint_id.unwrap_or(0)))
        } else {
            DatabaseReply::error()
        }
    }).map_err(|e| format!("Database query failed: {}", e))?;
    send_response(&reply, stream)
}

/*
 * Name: handle_authenticate
 * Function: Server logic for an authenrication request modelled by a state machine.
 */
fn handle_authenticate(
    request: DatabaseRequest,
    stream: &Arc<Mutex<TcpStream>>,
    client_id: usize,
    clients: &Arc<Mutex<HashMap<usize, Client>>>,
) -> Result<(), String> {
    let mut clients = clients.lock().unwrap();
    let client = clients.get_mut(&client_id).ok_or("Client not found")?;

    println!("Worker ID is {}", request.worker_id.unwrap());

    let response = match client.state {
        CheckpointState::WaitForRfid => {
            if authenticate_rfid(&request.worker_id, &request.checkpoint_id) {
                client.state = CheckpointState::WaitForFingerprint;
                DatabaseReply::auth_reply(CheckpointState::WaitForFingerprint)
            } else {
                client.state = CheckpointState::AuthFailed;
                DatabaseReply::auth_reply(CheckpointState::AuthFailed)
            }
        }
        CheckpointState::WaitForFingerprint => {
            if authenticate_fingerprint(&request.worker_id, &request.worker_fingerprint, &request.checkpoint_id) {
                client.state = CheckpointState::AuthSuccessful;
                DatabaseReply::auth_reply(CheckpointState::AuthSuccessful)
            } else {
                client.state = CheckpointState::AuthFailed;
                DatabaseReply::auth_reply(CheckpointState::AuthFailed)
            }
        }
        CheckpointState::AuthSuccessful | CheckpointState::AuthFailed => {
            thread::sleep(Duration::from_secs(5));
            client.state = CheckpointState::WaitForRfid;
            DatabaseReply::auth_reply(CheckpointState::WaitForRfid)
        }
    };

    send_response(&response, stream)
}

/*
 * Name: handle_database_request
 * Function: handles Update, Enroll and Delete requests from the cenralized database.
 */
fn handle_database_request(
    request: DatabaseRequest,
    stream: &Arc<Mutex<TcpStream>>,
    command: &str,
) -> Result<(), String> {
    let reply = query_database(DATABASE_ADDR, &request).map(|db_reply| {
        if db_reply.status == "success" {
            DatabaseReply::success(request.checkpoint_id)
        } else {
            DatabaseReply::error()
        }
    }).map_err(|e| format!("Database query failed: {}", e))?;
    send_response(&reply, stream)
}

/*
 * Name: send_response
 * Function: sends the result of the request back to the corresponding checkpoint.
 */
fn send_response<T: serde::Serialize>(
    response: &T,
    stream: &Arc<Mutex<TcpStream>>,
) -> Result<(), String> {
    let mut response_str = serde_json::to_string(response)
        .map_err(|e| format!("Failed to serialize response: {}", e))?;
    response_str.push('\0');
    stream.lock().unwrap()
        .write_all(response_str.as_bytes())
        .map_err(|e| format!("Failed to send response: {}", e))
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
