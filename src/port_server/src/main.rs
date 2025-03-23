/****************
    IMPORTS
****************/
use chrono::Local;
use common::{
    CheckpointReply, CheckpointState, Client, DatabaseReply, DatabaseRequest, Role, DATABASE_ADDR,
    SERVER_ADDR,
};
use ctrlc;
use rusqlite::{params, Connection, Result};
use std::fs::OpenOptions;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

const LOG_FILE: &str = "auth.log";

/**
 * Name: initialize_database
 * Function: Initializes a local employees database table for authentication.
 */
fn initialize_database() -> Result<Connection> {
    let conn = Connection::open("port_server_db.db")?;
    // TODO: Table containing employee IDs and their fingerprint IDs for each checkpoint

    conn.execute(
        "CREATE TABLE IF NOT EXISTS roles (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL
        )",
        [],
    )?;

    for (id, name) in Role::all_roles().iter().enumerate() {
        conn.execute(
            "INSERT OR IGNORE INTO roles (id, name) VALUES (?1, ?2)",
            params![id as i32, name],
        )?;
    }

    conn.execute(
        "CREATE TABLE IF NOT EXISTS employees (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            fingerprint_hash TEXT NOT NULL,
            role_id INTEGER NOT NULL,
            allowed_locations TEXT NOT NULL,
            FOREIGN KEY (role_id) REFERENCES roles (id)
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS checkpoints (
            id INTEGER PRIMARY KEY,
            location TEXT NOT NULL,
            allowed_roles TEXT NOT NULL
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS fingerprint_ids (
            employee_id INTEGER PRIMARY KEY,
        )",
        [],
    )?;

    Ok(conn)
}

/*
 * Name: check_local_db
 * Function: checks if a worker is in the local database.
 */
fn check_local_db(conn: &Connection, id: u32) -> Result<bool> {
    let mut stmt = conn.prepare("SELECT EXISTS(SELECT 1 FROM employees WHERE id = ?)")?;
    let exists: bool = stmt.query_row([id], |row| row.get(0))?;
    Ok(exists)
}

/*
 * Name: add_to_local_db
 * Function: adds a worker to the port server's database.
 */
fn add_to_local_db(
    conn: &Connection,
    id: u32,
    name: String,
    fingerprint_hash: String,
    role_id: i32,
    allowed_locations: String,
) -> Result<(), rusqlite::Error> {
    // Insert worker data into the employees table
    conn.execute(
        "INSERT INTO employees (id, name, fingerprint_hash, role_id, allowed_locations) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, name, fingerprint_hash, role_id, allowed_locations],
    )?;
    Ok(())
}

/*
 * Name: delete_from_local_db
 * Function: deletes a worker from the port server's database.
 */
fn delete_from_local_db(conn: &Connection, id: u32) -> Result<(), rusqlite::Error> {
    // Delete from employees table
    conn.execute("DELETE FROM employees WHERE id = ?1", params![id])?;
    Ok(())
}

/*
 * Name: update_worker_entry
 * Function: updates a worker's information in the local database.
 */
fn update_worker_entry(
    conn: &Connection,
    id: u32,
    locations: String,
    role: i32,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE employees SET role_id = ?1, allowed_locations = ?2 WHERE id = ?3",
        params![role, locations, id],
    )?;
    Ok(())
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
 * 1. Check local database.
 * 2. If exists check role.
 * 3. If it doesn't exist, check central database.
*  4. Retreive reply and check allowed locations.
*/
fn authenticate_rfid(
    conn: &Connection,
    rfid_tag: &Option<u32>,
    checkpoint_id: &Option<u32>,
) -> bool {
    if let (Some(rfid), Some(checkpoint)) = (rfid_tag, checkpoint_id) {
        if check_local_db(conn, *rfid).unwrap_or(false) {
            println!("Found worker in local database");
            let mut stmt = match conn.prepare(
                "SELECT roles.name
                 FROM employees
                 JOIN roles ON employees.role_id = roles.id
                 WHERE employees.id = ?",
            ) {
                Ok(stmt) => stmt,
                Err(_) => {
                    log_event(*rfid_tag, *checkpoint_id, "RFID", "Failed");
                    return false;
                }
            };

            let role_name: String = match stmt.query_row([rfid], |row| row.get(0)) {
                Ok(role) => role,
                Err(_) => {
                    log_event(*rfid_tag, *checkpoint_id, "RFID", "Failed");
                    return false;
                }
            };

            let mut stmt = match conn.prepare(
                "SELECT allowed_roles
                 FROM checkpoints
                 WHERE id = ?",
            ) {
                Ok(stmt) => stmt,
                Err(_) => {
                    log_event(*rfid_tag, *checkpoint_id, "RFID", "Failed");
                    return false;
                }
            };

            let allowed_roles: String = match stmt.query_row([checkpoint], |row| row.get(0)) {
                Ok(roles) => roles,
                Err(_) => {
                    log_event(*rfid_tag, *checkpoint_id, "RFID", "Failed");
                    return false;
                }
            };

            let allowed_roles_vec: Vec<String> = allowed_roles
                .split(',')
                .map(|role| role.trim().to_string())
                .collect();

            if !allowed_roles_vec.contains(&role_name) {
                log_event(*rfid_tag, *checkpoint_id, "RFID", "Failed");
                return false;
            } else {
                log_event(*rfid_tag, *checkpoint_id, "RFID", "Successful");
                return true;
            }
        }

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
                println!(
                    "RFID comparison: from checkpoint: {}, from database: {:?}",
                    rfid,
                    response.worker_id.unwrap_or(0)
                );
                println!("Response status: {}", response.status);

                if response.status == "error".to_string() {
                    log_event(Some(*rfid), Some(*checkpoint), "RFID", "Failed");
                    return false;
                }

                let authorized_roles: Vec<String> = response
                    .authorized_roles
                    .as_deref()
                    .unwrap_or("")
                    .split(',')
                    .map(|role| role.trim().to_string())
                    .collect();

                let role_str = Role::as_str(response.role_id.unwrap() as usize)
                    .unwrap()
                    .to_string();

                let allowed_locations_vec: Vec<String> = response
                    .allowed_locations
                    .as_deref()
                    .unwrap_or("")
                    .split(',')
                    .map(|loc| loc.trim().to_string())
                    .collect();

                let auth_successful = Some(rfid) == response.worker_id.as_ref()
                    && authorized_roles.contains(&role_str)
                    && allowed_locations_vec.contains(&response.location.clone().unwrap());

                println!("ID from DB: {}", response.worker_id.unwrap());
                println!("Checkpoint authorized roles: {:?}", authorized_roles);
                println!("User role: {}", role_str);
                println!("User allowed locations: {:?}", allowed_locations_vec);
                println!(
                    "Checkpoint location: {}",
                    response.location.clone().unwrap()
                );

                if auth_successful {
                    log_event(Some(*rfid), Some(*checkpoint), "RFID", "Successful");
                } else {
                    log_event(Some(*rfid), Some(*checkpoint), "RFID", "Failed");
                }

                return auth_successful;
            }
            Err(e) => {
                eprintln!("Error querying database for RFID: {:?}", e);
                log_event(*rfid_tag, *checkpoint_id, "RFID", "Failed");
                return false;
            }
        }
    } else {
        log_event(*rfid_tag, *checkpoint_id, "RFID", "Failed");
        return false;
    }
}

/*
 * Name: authenticate_fingerprint
 * Function: Similar to rfid with logic
*/
fn authenticate_fingerprint(
    conn: &Connection,
    rfid_tag: &Option<u32>,
    fingerprint_hash: &Option<String>,
    checkpoint_id: &Option<u32>,
) -> bool {
    if let (Some(rfid), Some(fingerprint), Some(checkpoint)) =
        (rfid_tag, fingerprint_hash, checkpoint_id)
    {
        if check_local_db(conn, *rfid).unwrap_or(false) {
            // Get stored fingerprint hash from local database
            let mut stmt = match conn.prepare(
                "SELECT fingerprint_hash
                 FROM employees
                 WHERE employees.id = ?",
            ) {
                Ok(stmt) => stmt,
                Err(_) => {
                    log_event(Some(*rfid), Some(*checkpoint), "Fingerprint", "Failed");
                    return false;
                }
            };

            let stored_fingerprint: String = match stmt.query_row([rfid], |row| row.get(0)) {
                Ok(fp) => fp,
                Err(_) => {
                    log_event(Some(*rfid), Some(*checkpoint), "Fingerprint", "Failed");
                    return false;
                }
            };

            if fingerprint == &stored_fingerprint {
                log_event(Some(*rfid), Some(*checkpoint), "Fingerprint", "Successful");
                return true;
            } else {
                log_event(Some(*rfid), Some(*checkpoint), "Fingerprint", "Failed");
                return false;
            }
        }

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
                println!(
                    "RFID comparison: from checkpoint: {}, from database: {:?}",
                    rfid, response.worker_id
                );
                println!(
                    "Fingerprint comparison: from checkpoint: {}, from database: {:?}",
                    fingerprint, response.worker_fingerprint
                );

                if response.status != "success".to_string() {
                    log_event(Some(*rfid), Some(*checkpoint), "Fingerprint", "Failed");
                    return false;
                }

                let auth = Some(rfid) == response.worker_id.as_ref()
                    && Some(fingerprint) == response.worker_fingerprint.as_ref();

                if auth {
                    match add_to_local_db(
                        conn,
                        response.worker_id.unwrap(),
                        response.worker_name.unwrap(),
                        response.worker_fingerprint.unwrap(),
                        response.role_id.unwrap() as i32,
                        response.allowed_locations.unwrap(),
                    ) {
                        Ok(_) => {
                            log_event(Some(*rfid), Some(*checkpoint), "Fingerprint", "Successful");
                            return true;
                        }
                        Err(e) => {
                            eprintln!(
                                "An error occurred with adding the user to the database : {}",
                                e
                            );
                            log_event(Some(*rfid), Some(*checkpoint), "Fingerprint", "Failed");
                            return true;
                        }
                    }
                } else {
                    log_event(Some(*rfid), Some(*checkpoint), "Fingerprint", "Failed");
                    return false;
                }
            }
            Err(e) => {
                eprintln!("Error querying database for fingerprint hash: {}", e);
                log_event(Some(*rfid), Some(*checkpoint), "Fingerprint", "Failed");
                return false;
            }
        }
    } else {
        log_event(*rfid_tag, *checkpoint_id, "Fingerprint", "Failed");
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
    thread::sleep(Duration::from_secs(1));
    let request_json = serde_json::to_string(request)
        .map_err(|e| format!("Failed to serialize request: {}", e))?;

    let mut stream = TcpStream::connect(database_addr)
        .map_err(|e| format!("Failed to connect to database: {}", e))?;

    stream
        .write_all(format!("{}", request_json).as_bytes())
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
    conn: Arc<Mutex<Connection>>,
    stream: Arc<Mutex<TcpStream>>,
    client_id: usize,
    clients: Arc<Mutex<HashMap<usize, Client>>>,
    running: Arc<AtomicBool>,
) {
    println!("Client {} connected", client_id);

    let mut reader = BufReader::new(stream.lock().unwrap().try_clone().unwrap());
    let mut buffer = Vec::new();

    while running.load(Ordering::SeqCst) {
        match read_request(
            &conn,
            &mut reader,
            &stream,
            client_id,
            &clients,
            &mut buffer,
        ) {
            Ok(_) => continue,
            Err(e) if e.contains("WouldBlock") => {
                thread::sleep(Duration::from_millis(50)); // Small sleep before retrying
                continue;
            }
            Err(e) => {
                eprintln!("Error processing client {}: {}", client_id, e);
                break;
            }
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
    conn: &Arc<Mutex<Connection>>,
    reader: &mut BufReader<TcpStream>,
    stream: &Arc<Mutex<TcpStream>>,
    client_id: usize,
    clients: &Arc<Mutex<HashMap<usize, Client>>>,
    buffer: &mut Vec<u8>,
) -> Result<(), String> {
    println!("Received a request");
    buffer.clear();
    match reader.read_until(b'\0', buffer) {
        Ok(0) => Err("Client disconnected".into()),
        Ok(_) => {
            buffer.pop();
            let request_str = parse_request(buffer)?;
            let request: DatabaseRequest = serde_json::from_str(&request_str)
                .map_err(|e| format!("Failed to parse request: {}", e))?;
            parse_command_from_request(conn, request, stream, client_id, clients)?;
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
    conn: &Arc<Mutex<Connection>>,
    request: DatabaseRequest,
    stream: &Arc<Mutex<TcpStream>>,
    client_id: usize,
    clients: &Arc<Mutex<HashMap<usize, Client>>>,
) -> Result<(), String> {
    match request.command.as_str() {
        "INIT_REQUEST" => handle_init_request(conn, request, stream),
        "AUTHENTICATE" => handle_authenticate(conn, request, stream, client_id, clients),
        "ENROLL" => {
            let conn = conn.lock().unwrap(); // Lock the Mutex to get &Connection
            handle_database_request(&conn, request, stream)
        }
        "UPDATE" => {
            let conn = conn.lock().unwrap(); // Lock the Mutex to get &Connection
            handle_database_request(&conn, request, stream)
        }
        "DELETE" => {
            let conn = conn.lock().unwrap(); // Lock the Mutex to get &Connection
            handle_database_request(&conn, request, stream)
        }
        _ => Err("Unknown command".into()),
    }
}

/*
 * Name: handle_init_request
 * Function: Handler for a checkpoint init_request.
 */
fn handle_init_request(
    conn: &Arc<Mutex<Connection>>,
    request: DatabaseRequest,
    stream: &Arc<Mutex<TcpStream>>,
) -> Result<(), String> {
    println!("Received INIT request");
    conn.lock()
        .unwrap()
        .execute(
            "INSERT INTO checkpoints (location, allowed_roles) VALUES (?1, ?2)",
            params![request.location, request.authorized_roles],
        )
        .map_err(|e| format!("Failed to insert checkpoint: {}", e))?;

    let reply = query_database(DATABASE_ADDR, &request)
        .map(|db_reply| {
            if db_reply.status == "success" {
                println!("Got checkpoint ID: {}", db_reply.checkpoint_id.unwrap());
                DatabaseReply::init_reply(db_reply.checkpoint_id.unwrap())
            } else {
                println!("Database returned an error");
                DatabaseReply::error()
            }
        })
        .map_err(|e| format!("Database query failed: {}", e))?;
    send_response(&reply, stream)
}

/*
 * Name: handle_authenticate
 * Function: Server logic for an authentication request modelled by a state machine.
 */
fn handle_authenticate(
    conn: &Arc<Mutex<Connection>>,
    request: DatabaseRequest,
    stream: &Arc<Mutex<TcpStream>>,
    client_id: usize,
    clients: &Arc<Mutex<HashMap<usize, Client>>>,
) -> Result<(), String> {
    let mut clients = clients.lock().unwrap();
    let client = clients.get_mut(&client_id).ok_or("Client not found")?;

    println!("Worker ID is {}", request.worker_id.unwrap());

    let next_state: CheckpointState;
    let response = match client.state {
        CheckpointState::WaitForRfid => {
            match query_database(DATABASE_ADDR, &request) {
                Ok(db_response) => {
                    let scanned_worker_id = request.worker_id; // ID from Checkpoint
                    let db_worker_id = db_response.worker_id; // ID from Database

                    println!(
                        "Checkpoint Scanned Worker ID: {:?}, Database Worker ID: {:?}",
                        scanned_worker_id, db_worker_id
                    );

                    if scanned_worker_id.is_some()
                        && db_worker_id.is_some()
                        && scanned_worker_id == db_worker_id
                    {
                        println!(
                            "RFID Verified: {:?} matches database entry.",
                            scanned_worker_id
                        );

                        next_state = CheckpointState::WaitForFingerprint;
                        CheckpointReply {
                            status: "success".to_string(),
                            checkpoint_id: request.checkpoint_id,
                            worker_id: scanned_worker_id,
                            fingerprint: None,
                            data: None,
                            auth_response: Some(CheckpointState::WaitForFingerprint),
                            rfid_ver: Some(true),
                        }
                    } else {
                        eprintln!(
                            "RFID Mismatch: Checkpoint ID: {:?}, Database ID: {:?}",
                            scanned_worker_id, db_worker_id
                        );
                        next_state = CheckpointState::AuthFailed;
                        CheckpointReply {
                            status: "failed".to_string(),
                            checkpoint_id: request.checkpoint_id,
                            worker_id: None,
                            fingerprint: None,
                            data: None,
                            auth_response: Some(CheckpointState::AuthFailed),
                            rfid_ver: Some(false),
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error querying database: {}", e);
                    next_state = CheckpointState::AuthFailed;
                    CheckpointReply {
                        status: "error".to_string(),
                        checkpoint_id: request.checkpoint_id,
                        worker_id: None,
                        fingerprint: None,
                        data: None,
                        auth_response: Some(CheckpointState::AuthFailed),
                        rfid_ver: Some(false),
                    }
                }
            }
        }
        CheckpointState::WaitForFingerprint => {
            if authenticate_fingerprint(
                &conn.lock().unwrap(),
                &request.worker_id,
                &request.worker_fingerprint,
                &request.checkpoint_id,
            ) {
                next_state = CheckpointState::AuthSuccessful;
                CheckpointReply::auth_reply(CheckpointState::AuthSuccessful)
            } else {
                next_state = CheckpointState::AuthFailed;
                CheckpointReply::auth_reply(CheckpointState::AuthFailed)
            }
        }
        CheckpointState::AuthSuccessful | CheckpointState::AuthFailed => {
            next_state = CheckpointState::WaitForRfid;
            CheckpointReply::auth_reply(CheckpointState::WaitForRfid)
        }
    };
    client.state = next_state;

    send_response(&response, stream)
}

/*
 * Name: handle_database_request
 * Function: handles Update, Enroll and Delete requests from the centralized database.
 */
fn handle_database_request(
    conn: &Connection,
    request: DatabaseRequest,
    stream: &Arc<Mutex<TcpStream>>,
) -> Result<(), String> {
    // Query the central database
    let db_reply = query_database(DATABASE_ADDR, &request)
        .map_err(|e| format!("Database query failed: {}", e))?;

    // Process the reply based on the command
    let reply = if db_reply.status == "success" {
        match request.command.as_str() {
            "ENROLL" => DatabaseReply::success(db_reply.worker_id.unwrap()),
            "DELETE" => {
                // Safely unwrap worker_id or return an error
                let worker_id = request
                    .worker_id
                    .ok_or("Worker ID is missing in the request")?;

                // Check if the worker exists in the local database
                if check_local_db(conn, worker_id).map_err(|e| format!("Database error: {}", e))? {
                    // Delete the worker from the local database
                    match delete_from_local_db(conn, worker_id) {
                        Ok(_) => {
                            DatabaseReply::init_reply(request.checkpoint_id.unwrap_or_default())
                        }
                        Err(e) => {
                            eprintln!("Failed to delete worker: {}", e);
                            DatabaseReply::error()
                        }
                    }
                } else {
                    // Worker not found in the local database
                    eprintln!(
                        "Worker with ID {} not found in the local database",
                        worker_id
                    );
                    DatabaseReply::error()
                }
            }
            "UPDATE" => {
                // Safely unwrap worker_id, role_id, and allowed_locations or return an error
                let worker_id = request
                    .worker_id
                    .ok_or("Worker ID is missing in the request")?;
                request.role_id.ok_or("Role ID is missing in the request")?;
                request
                    .location
                    .ok_or("Allowed locations are missing in the request")?;

                // Check if the worker exists in the local database
                if check_local_db(conn, worker_id).map_err(|e| format!("Database error: {}", e))? {
                    // Update the worker's role and allowed locations
                    match update_worker_entry(
                        conn,
                        request.worker_id.unwrap(),
                        db_reply.allowed_locations.unwrap(),
                        db_reply.role_id.unwrap() as i32,
                    ) {
                        Ok(_) => {
                            DatabaseReply::init_reply(request.checkpoint_id.unwrap_or_default())
                        }
                        Err(e) => {
                            eprintln!("Failed to update worker entry: {}", e);
                            DatabaseReply::error()
                        }
                    }
                } else {
                    // Worker not found in the local database
                    eprintln!(
                        "Worker with ID {} not found in the local database",
                        worker_id
                    );
                    DatabaseReply::error()
                }
            }
            _ => DatabaseReply::init_reply(request.checkpoint_id.unwrap_or_default()),
        }
    } else {
        // Central database query failed
        DatabaseReply::error()
    };

    // Send the response back to the client
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
    stream
        .lock()
        .unwrap()
        .write_all(response_str.as_bytes())
        .map_err(|e| format!("Failed to send response: {}", e))
}

// Writes log entry to `auth.log`
fn log_event(worker_id: Option<u32>, checkpoint_id: Option<u32>, method: &str, status: &str) {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let log_entry = format!(
        "[{}] Worker ID: {:?}, Checkpoint ID: {:?}, Method: {}, Status: {}\n",
        timestamp, worker_id, checkpoint_id, method, status
    );

    let mut file = match OpenOptions::new().create(true).append(true).open(LOG_FILE) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open {}: {}", LOG_FILE, e);
            return;
        }
    };

    if let Err(e) = writeln!(file, "{}", log_entry) {
        eprintln!("Failed to write to auth.log: {}", e);
    }

    match method {
        "RFID" | "Fingerprint" => {
            if status == "Successful" {
                println!(
                    "[LOG] Authentication success: Worker {} at Checkpoint {}",
                    worker_id.unwrap_or(0),
                    checkpoint_id.unwrap_or(0)
                );
            } else {
                println!(
                    "[LOG] Authentication failed: Worker {} at Checkpoint {}",
                    worker_id.unwrap_or(0),
                    checkpoint_id.unwrap_or(0)
                );
            }
        }
        "RoleChange" => {
            println!(
                "[LOG] Role changed for Worker {} to {}",
                worker_id.unwrap_or(0),
                status
            );
        }
        "AdminAuth" => {
            println!("[LOG] Admin authenticated: {}", worker_id.unwrap_or(0));
        }
        _ => {}
    }
}
// Main server function
fn main() -> Result<(), rusqlite::Error> {
    let listener = TcpListener::bind(SERVER_ADDR).expect("Failed to bind address");
    listener
        .set_nonblocking(false)
        .expect("Cannot set non-blocking mode");
    println!("Server listening on {}", SERVER_ADDR);

    let clients: Arc<Mutex<HashMap<usize, Client>>> = Arc::new(Mutex::new(HashMap::new()));
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = Arc::clone(&running);

    // Database Initialization
    let database = initialize_database()?;
    let database = Arc::new(Mutex::new(database));

    let mut client_id_counter = 0;

    while running.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, addr)) => {
                println!(
                    "New client connected: {} with ID {}",
                    addr, client_id_counter
                );

                set_stream_timeout(&stream, Duration::from_secs(300));
                let stream = Arc::new(Mutex::new(stream));

                let client_id = client_id_counter;
                client_id_counter += 1;

                let clients = Arc::clone(&clients);
                let running = Arc::clone(&running);
                let database = Arc::clone(&database);

                clients.lock().unwrap().insert(
                    client_id,
                    Client {
                        id: client_id,
                        stream: Arc::clone(&stream),
                        state: CheckpointState::WaitForRfid,
                    },
                );

                thread::spawn(move || handle_client(database, stream, client_id, clients, running));
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    // Helper function to initialize the database with test data
    fn setup_test_database() -> Connection {
        let conn = Connection::open(":memory:").expect("Failed to create in-memory database");

        // Create tables
        conn.execute(
            "CREATE TABLE roles (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL
            )",
            [],
        )
        .expect("Failed to create roles table");

        conn.execute(
            "CREATE TABLE employees (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                fingerprint_hash TEXT NOT NULL,
                role_id INTEGER NOT NULL,
                allowed_locations TEXT NOT NULL,
                FOREIGN KEY (role_id) REFERENCES roles (id)
            )",
            [],
        )
        .expect("Failed to create employees table");

        conn.execute(
            "CREATE TABLE checkpoints (
                id INTEGER PRIMARY KEY,
                location TEXT NOT NULL,
                allowed_roles TEXT NOT NULL
            )",
            [],
        )
        .expect("Failed to create checkpoints table");

        // Insert test roles
        conn.execute(
            "INSERT INTO roles (id, name) VALUES (1, 'Admin'), (2, 'Worker')",
            [],
        )
        .expect("Failed to insert roles");

        // Insert test employees
        conn.execute(
            "INSERT INTO employees (id, name, fingerprint_hash, role_id, allowed_locations) VALUES 
            (1, 'John Doe', 'hash1', 1, 'Location1,Location2'),
            (2, 'Jane Doe', 'hash2', 2, 'Location2')",
            [],
        )
        .expect("Failed to insert employees");

        // Insert test checkpoints
        conn.execute(
            "INSERT INTO checkpoints (id, location, allowed_roles) VALUES 
            (1, 'Location1', 'Admin'),
            (2, 'Location2', 'Worker')",
            [],
        )
        .expect("Failed to insert checkpoints");

        conn
    }

    #[test]
    fn test_initialize_database() {
        let conn = initialize_database().expect("Failed to initialize database");

        // Check if tables are created
        let roles_table_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='roles')",
                [],
                |row| row.get(0),
            )
            .expect("Failed to query roles table existence");
        assert!(roles_table_exists);

        let employees_table_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='employees')",
                [],
                |row| row.get(0),
            )
            .expect("Failed to query employees table existence");
        assert!(employees_table_exists);

        let checkpoints_table_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='checkpoints')",
                [],
                |row| row.get(0),
            )
            .expect("Failed to query checkpoints table existence");
        assert!(checkpoints_table_exists);
    }

    #[test]
    fn test_check_local_db() {
        let conn = setup_test_database();

        // Test existing employee
        let exists = check_local_db(&conn, 1).expect("Failed to check local database");
        assert!(exists);

        // Test non-existing employee
        let exists = check_local_db(&conn, 999).expect("Failed to check local database");
        assert!(!exists);
    }

    #[test]
    fn test_add_to_local_db() {
        let conn = setup_test_database();

        // Add a new employee
        add_to_local_db(
            &conn,
            3,
            "New Employee".to_string(),
            "hash3".to_string(),
            2,
            "Location2".to_string(),
        )
        .expect("Failed to add to local database");

        // Check if the employee was added
        let exists = check_local_db(&conn, 3).expect("Failed to check local database");
        assert!(exists);
    }

    #[test]
    fn test_delete_from_local_db() {
        let conn = setup_test_database();

        // Delete an existing employee
        delete_from_local_db(&conn, 1).expect("Failed to delete from local database");

        // Check if the employee was deleted
        let exists = check_local_db(&conn, 1).expect("Failed to check local database");
        assert!(!exists);
    }

    #[test]
    fn test_update_worker_entry() {
        let conn = setup_test_database();

        // Update an existing employee
        update_worker_entry(&conn, 1, "Location3".to_string(), 2)
            .expect("Failed to update worker entry");

        // Check if the employee was updated
        let mut stmt = conn
            .prepare("SELECT allowed_locations, role_id FROM employees WHERE id = ?")
            .expect("Failed to prepare statement");
        let (locations, role_id): (String, i32) = stmt
            .query_row(params![1], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("Failed to query updated employee");
        assert_eq!(locations, "Location3");
        assert_eq!(role_id, 2);
    }

    #[test]
    fn test_authenticate_rfid() {
        let conn = setup_test_database();

        // Test valid RFID and checkpoint
        let mock_tag: Option<u32> = Some(1);
        let mock_checkpoint: Option<u32> = Some(1);
        let result = authenticate_rfid(&conn, &mock_tag, &mock_checkpoint);
        assert!(result);

        // Test invalid RFID (wrong role for checkpoint)
        let mock_tag_mismatch_role: Option<u32> = Some(2);
        let result_mismatch_role =
            authenticate_rfid(&conn, &mock_tag_mismatch_role, &mock_checkpoint);
        assert!(!result_mismatch_role);

        // Test invalid RFID (non-existent)
        let mock_tag_invalid: Option<u32> = Some(999);
        let result_invalid = authenticate_rfid(&conn, &mock_tag_invalid, &mock_checkpoint);
        assert!(!result_invalid);
    }

    #[test]
    fn test_authenticate_fingerprint() {
        let conn = setup_test_database();

        // Test valid fingerprint
        let mock_tag: Option<u32> = Some(1);
        let mock_fingerprint: Option<String> = Some("hash1".to_string());
        let mock_checkpoint: Option<u32> = Some(1);
        let result =
            authenticate_fingerprint(&conn, &mock_tag, &mock_fingerprint, &mock_checkpoint);
        assert!(result);

        // Test invalid fingerprint
        let mock_fingerprint_invalid: Option<String> = Some("wrong_hash".to_string());
        let result_invalid = authenticate_fingerprint(
            &conn,
            &mock_tag,
            &mock_fingerprint_invalid,
            &mock_checkpoint,
        );
        assert!(!result_invalid);

        // Test invalid RFID
        let mock_tag_invalid: Option<u32> = Some(999);
        let result_invalid_rfid = authenticate_fingerprint(
            &conn,
            &mock_tag_invalid,
            &mock_fingerprint,
            &mock_checkpoint,
        );
        assert!(!result_invalid_rfid);
    }
}
