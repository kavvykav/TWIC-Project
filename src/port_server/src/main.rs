/****************
    IMPORTS
****************/
use ctrlc;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use common::{
    CheckpointReply, CheckpointState, Client, DatabaseReply, DatabaseRequest, Role, DATABASE_ADDR, SERVER_ADDR
};


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
                let authorized_roles: Vec<String> = response.authorized_roles
                    .as_deref() // Converts Option<String> to Option<&str>
                    .unwrap_or("") // If None, provide a default empty string
                    .split(',')
                    .map(|role| role.trim().to_string())
                    .collect();
                            
                let role_str = Role::as_str(response.role_id.unwrap() as usize).unwrap().to_string();
                            
                            
                let allowed_locations_vec: Vec<String> = response.allowed_locations
                    .as_deref()
                    .unwrap_or("")
                    .split(',')
                    .map(|loc| loc.trim().to_string())
                    .collect();
                            
                return Some(rfid) == response.worker_id.as_ref() && // check IDs match up
                       authorized_roles.contains(&role_str) && // check role is allowed at checkpoint
                       allowed_locations_vec.contains(&response.location.unwrap()); // check worker is allowed at that port                                      
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
        "ENROLL" => handle_database_request(request, stream),
        "UPDATE" => handle_database_request(request, stream),
        "DELETE" => handle_database_request(request, stream),
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
            DatabaseReply::init_reply(db_reply.checkpoint_id.unwrap())
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
                CheckpointReply::auth_reply(CheckpointState::WaitForFingerprint)
            } else {
                client.state = CheckpointState::AuthFailed;
                CheckpointReply::auth_reply(CheckpointState::AuthFailed)
            }
        }
        CheckpointState::WaitForFingerprint => {
            if authenticate_fingerprint(&request.worker_id, &request.worker_fingerprint, &request.checkpoint_id) {
                client.state = CheckpointState::AuthSuccessful;
                CheckpointReply::auth_reply(CheckpointState::AuthSuccessful)
            } else {
                client.state = CheckpointState::AuthFailed;
                CheckpointReply::auth_reply(CheckpointState::AuthFailed)
            }
        }
        CheckpointState::AuthSuccessful | CheckpointState::AuthFailed => {
            thread::sleep(Duration::from_secs(5));
            client.state = CheckpointState::WaitForRfid;
            CheckpointReply::auth_reply(CheckpointState::WaitForRfid)
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
) -> Result<(), String> {
    let reply = query_database(DATABASE_ADDR, &request).map(|db_reply| {
        if db_reply.status == "success" {
            DatabaseReply::init_reply(request.checkpoint_id.unwrap())
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
