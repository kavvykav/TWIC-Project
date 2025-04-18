/****************
    IMPORTS
****************/
use base64::{engine::general_purpose, Engine as _};
use chrono::Local;
use common::{
    decrypt_string, encrypt_aes, keygen_string, CheckpointReply, CheckpointState, Client,
    DatabaseReply, DatabaseRequest, Parameters, Role, DATABASE_ADDR, SERVER_ADDR,
};
use lazy_static::lazy_static;
use rusqlite::{params, Connection, Result};
use std::fs::OpenOptions;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, ErrorKind::WouldBlock, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const LOG_FILE: &str = "auth.log";

lazy_static! {
    static ref PS_KEYPAIR: Mutex<HashMap<String, String>> = Mutex::new({
        let params = Parameters::default();
        let keypair = keygen_string(&params, None);
        println!("Port Server Public Key: {}", keypair.get("public").unwrap());
        keypair
    });
    static ref SYMMETRIC_KEY: Mutex<Option<String>> = Mutex::new(None);
    static ref SYMMETRIC_IV: Mutex<Option<String>> = Mutex::new(None);
}

/**
 * Initialize database with simplified schema (fingerprint_id as INTEGER)
 */
fn initialize_database() -> Result<Connection> {
    let conn = Connection::open("port_server_db.db")?;

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
            fingerprint_id INTEGER NOT NULL,
            role_id INTEGER NOT NULL,
            allowed_locations TEXT NOT NULL,
            rfid_data INTEGER NOT NULL,
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
        "INSERT OR IGNORE INTO checkpoints (id, location, allowed_roles) VALUES 
        (999, 'AdminSystem', 'Admin')",
        [],
    )?;

    Ok(conn)
}

/**
 * Simplified database operations using integer fingerprint IDs
 */
fn check_local_db(conn: &Connection, id: u64) -> Result<bool> {
    let mut stmt = conn.prepare("SELECT EXISTS(SELECT 1 FROM employees WHERE id = ?)")?;
    let exists: bool = stmt.query_row([id], |row| row.get(0))?;
    Ok(exists)
}

fn add_to_local_db(
    conn: &Connection,
    id: u64,
    name: String,
    fingerprint_id: u32,
    role_id: i32,
    allowed_locations: String,
    rfid_data: u32,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT OR REPLACE INTO employees (id, name, fingerprint_id, role_id, allowed_locations, rfid_data) 
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, name, fingerprint_id, role_id, allowed_locations, rfid_data],
    )?;
    Ok(())
}

fn delete_from_local_db(conn: &Connection, id: u64) -> Result<(), rusqlite::Error> {
    conn.execute("DELETE FROM employees WHERE id = ?1", params![id])?;
    Ok(())
}

fn update_worker_entry(
    conn: &Connection,
    id: u64,
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
    rfid_tag: &Option<u64>,
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
                    log_event(Some(*rfid), Some(*checkpoint), "RFID", "Failed");
                    return false;
                }
            };

            let role_name: String = match stmt.query_row([rfid], |row| row.get(0)) {
                Ok(role) => role,
                Err(_) => {
                    log_event(
                        rfid_tag.map(|id| id.into()),
                        checkpoint_id.map(|id| id.into()),
                        "RFID",
                        "Failed",
                    );
                    return false;
                }
            };

            let mut stmt = match conn.prepare(
                "SELECT allowed_roles
                 FROM checkpoints
                 WHERE id = ?",
            ) {
                Ok(stmt) => stmt,
                Err(e) => {
                    eprintln!("Query failed: {}", e);
                    log_event(
                        rfid_tag.map(|id| id.into()),
                        checkpoint_id.map(|id| id.into()),
                        "RFID",
                        "Failed",
                    );
                    return false;
                }
            };

            let allowed_roles: String = match stmt.query_row([checkpoint], |row| row.get(0)) {
                Ok(roles) => roles,
                Err(e) => {
                    eprintln!("Role query failed: {}", e);
                    log_event(
                        rfid_tag.map(|id| id.into()),
                        checkpoint_id.map(|id| id.into()),
                        "RFID",
                        "Failed",
                    );
                    return false;
                }
            };

            let allowed_roles_vec: Vec<String> = allowed_roles
                .split(',')
                .map(|role| role.trim().to_string())
                .collect();

            if !allowed_roles_vec.contains(&role_name) {
                println!("User does not have the required role");
                log_event(
                    rfid_tag.map(|id| id.into()),
                    checkpoint_id.map(|id| id.into()),
                    "RFID",
                    "Failed",
                );
                return false;
            } else {
                log_event(
                    rfid_tag.map(|id| id.into()),
                    checkpoint_id.map(|id| id.into()),
                    "RFID",
                    "Successful",
                );
                return true;
            }
        }

        let request = DatabaseRequest {
            command: "AUTHENTICATE".to_string(),
            checkpoint_id: Some(*checkpoint),
            worker_id: Some(*rfid),
            rfid_data: None,
            worker_fingerprint: None,
            location: None,
            authorized_roles: None,
            worker_name: None,
            role_id: None,
            encrypted_aes_key: None,
            encrypted_iv: None,
            public_key: None,
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
                    log_event(
                        Some(u64::from(*rfid)),
                        Some(u32::from(*checkpoint)),
                        "RFID",
                        "Failed",
                    );
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

                let auth_successful = Some(u64::from(*rfid))
                    == response.worker_id.map(|id| u64::from(id))
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
                    log_event(
                        Some(u64::from(*rfid)),
                        Some(u32::from(*checkpoint)),
                        "RFID",
                        "Successful",
                    );
                } else {
                    log_event(
                        Some(u64::from(*rfid)),
                        Some(u32::from(*checkpoint)),
                        "RFID",
                        "Failed",
                    );
                }

                return auth_successful;
            }
            Err(e) => {
                eprintln!("Error querying database for RFID: {:?}", e);
                log_event(
                    rfid_tag.map(|id| id.into()),
                    checkpoint_id.map(|id| id.into()),
                    "RFID",
                    "Failed",
                );
                return false;
            }
        }
    } else {
        log_event(
            rfid_tag.map(|id| id.into()),
            checkpoint_id.map(|id| id.into()),
            "RFID",
            "Failed",
        );
        return false;
    }
}

/*
 * Name: authenticate_fingerprint
 * Function: Similar to rfid with logic
*/
fn authenticate_fingerprint(
    conn: &Connection,
    rfid_tag: &Option<u64>,
    fingerprint_id: &Option<String>,
    checkpoint_id: &Option<u32>,
) -> bool {
    let (rfid, fingerprint_str, checkpoint) = match (rfid_tag, fingerprint_id, checkpoint_id) {
        (Some(r), Some(f), Some(c)) => (r, f, c),
        _ => {
            log_event(
                rfid_tag.map(|id| id.into()),
                checkpoint_id.map(|id| id.into()),
                "Fingerprint",
                "Failed - Missing data",
            );
            return false;
        }
    };

    let fingerprint: u32 = match fingerprint_str.parse() {
        Ok(id) => id,
        Err(_) => {
            log_event(
                Some(*rfid),
                Some(*checkpoint),
                "Fingerprint",
                "Failed - Invalid format",
            );
            return false;
        }
    };

    if check_local_db(conn, *rfid).unwrap_or(false) {
        match conn.query_row(
            "SELECT fingerprint_id FROM employees WHERE id = ?",
            [rfid],
            |row| row.get::<_, u32>(0),
        ) {
            Ok(db_fingerprint) => {
                let auth_successful = db_fingerprint == fingerprint;
                log_event(
                    Some(*rfid),
                    Some(*checkpoint),
                    "Fingerprint",
                    if auth_successful {
                        "Success"
                    } else {
                        "Failed - No Match"
                    },
                );
                auth_successful
            }
            Err(e) => {
                eprintln!("Fingerprint query failed: {}", e);
                log_event(
                    Some(*rfid),
                    Some(*checkpoint),
                    "Fingerprint",
                    "Failed - DB Error",
                );
                false
            }
        }
    } else {
        // Central database fallback logic remains the same
        let request = DatabaseRequest {
            command: "AUTHENTICATE".to_string(),
            checkpoint_id: Some(*checkpoint),
            worker_id: Some(*rfid),
            rfid_data: None,
            worker_fingerprint: Some(fingerprint_str.clone()),
            location: None,
            authorized_roles: None,
            worker_name: None,
            role_id: None,
            encrypted_aes_key: None,
            encrypted_iv: None,
            public_key: None,
        };

        match query_database(DATABASE_ADDR, &request) {
            Ok(response) if response.status == "success" => {
                if let (Some(db_rfid), Some(db_fingerprint)) =
                    (response.worker_id, response.worker_fingerprint)
                {
                    let auth = *rfid == db_rfid && fingerprint_str == &db_fingerprint.to_string();
                    if auth {
                        // Add to local cache
                        if let (
                            Some(id),
                            Some(name),
                            Some(fp),
                            Some(role),
                            Some(locations),
                            Some(rfid_data),
                        ) = (
                            response.worker_id,
                            response.worker_name,
                            response.worker_fingerprint,
                            response.role_id,
                            response.allowed_locations,
                            response.rfid_data,
                        ) {
                            let _ = add_to_local_db(
                                conn,
                                id,
                                name,
                                fp,
                                role as i32,
                                locations,
                                rfid_data,
                            );
                        }
                    }
                    log_event(
                        Some(*rfid),
                        Some(*checkpoint),
                        "Fingerprint",
                        if auth { "Success" } else { "Failed - Mismatch" },
                    );
                    auth
                } else {
                    false
                }
            }
            _ => {
                log_event(
                    Some(*rfid),
                    Some(*checkpoint),
                    "Fingerprint",
                    "Failed - DB error",
                );
                false
            }
        }
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

    let aes_key_opt = SYMMETRIC_KEY.lock().unwrap().clone();
    let aes_iv_opt = SYMMETRIC_IV.lock().unwrap().clone();

    let encrypted_request =
        if aes_key_opt.is_some() && aes_iv_opt.is_some() && request.command != "KEY_EXCHANGE" {
            let aes_key = hex::decode(aes_key_opt.unwrap()).expect("Invalid AES Key");
            let aes_iv = hex::decode(aes_iv_opt.unwrap()).expect("Invalid IV");

            println!("Encrypting request before sending to database...");
            encrypt_aes(&request_json, &aes_key, &aes_iv)
        } else {
            println!("WARNING: Sending unencrypted request ({})", request.command);
            request_json.as_bytes().to_vec()
        };

    let mut stream = TcpStream::connect(database_addr)
        .map_err(|e| format!("Failed to connect to database: {}", e))?;

    stream
        .write_all(&encrypted_request)
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
 * Name: key_exchange
 * Function: Begins the key exchange process with database, sends over key and iv values
 */
fn key_exchange() -> bool {
    let ps_keypair = PS_KEYPAIR.lock().unwrap();
    let my_public_key = ps_keypair
        .get("public")
        .expect("Public key not found")
        .clone();
    println!("{}", my_public_key);
    drop(ps_keypair); // Release the lock early.

    let request = DatabaseRequest {
        command: "KEY_EXCHANGE".to_string(),
        checkpoint_id: None,
        worker_id: None,
        rfid_data: None,
        worker_name: None,
        worker_fingerprint: None,
        location: None,
        authorized_roles: None,
        role_id: None,
        encrypted_aes_key: None,
        encrypted_iv: None,
        public_key: Some(my_public_key),
    };

    match query_database(DATABASE_ADDR, &request) {
        Ok(reply) => {
            if reply.status == "success" {
                if let (Some(encrypted_aes_key), Some(encrypted_iv)) =
                    (reply.encrypted_aes_key, reply.encrypted_iv)
                {
                    let ps_keypair = PS_KEYPAIR.lock().unwrap();
                    let my_private_key = ps_keypair.get("secret").expect("Private key not found");
                    let rlwe_params = Parameters::default();

                    let decrypted_aes_key =
                        decrypt_string(my_private_key, &encrypted_aes_key, &rlwe_params);
                    let decrypted_iv = decrypt_string(my_private_key, &encrypted_iv, &rlwe_params);

                    println!("Decrypted AES Key: {:?}", decrypted_aes_key);
                    println!("Decrypted IV: {:?}", decrypted_iv);

                    SYMMETRIC_KEY
                        .lock()
                        .unwrap()
                        .replace(general_purpose::STANDARD.encode(&decrypted_aes_key));
                    SYMMETRIC_IV
                        .lock()
                        .unwrap()
                        .replace(general_purpose::STANDARD.encode(&decrypted_iv));

                    return true;
                } else {
                    eprintln!("Key exchange reply is missing encrypted keys.");
                    return false;
                }
            } else {
                eprintln!("Key exchange failed: status not 'success'.");
                return false;
            }
        }
        Err(e) => {
            eprintln!("Error during key exchange: {:?}", e);
            return false;
        }
    }
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
    let mut last_state_change = Instant::now();
    let mut previous_state = CheckpointState::WaitForRfid;

    while running.load(Ordering::SeqCst) {
        // Check state and timer outside of the clients_lock scope
        let should_timeout = {
            let clients_lock = clients.lock().unwrap();
            if let Some(client) = clients_lock.get(&client_id) {
                // If state just changed to WaitForFingerprint, reset timer
                if client.state == CheckpointState::WaitForFingerprint
                    && previous_state != CheckpointState::WaitForFingerprint
                {
                    last_state_change = Instant::now();
                }

                // Check if timeout occurred
                client.state == CheckpointState::WaitForFingerprint
                    && last_state_change.elapsed() > Duration::from_secs(15)
            } else {
                false
            }
        };

        // If timeout occurred, update state
        if should_timeout {
            println!(
                "Client {} timed out waiting for fingerprint, transitioning to WaitForRfid",
                client_id
            );

            let mut clients_lock = clients.lock().unwrap();
            if let Some(client) = clients_lock.get_mut(&client_id) {
                client.state = CheckpointState::WaitForRfid;
            }
        }

        // Update previous state
        {
            let clients_lock = clients.lock().unwrap();
            if let Some(client) = clients_lock.get(&client_id) {
                previous_state = client.state.clone();
            }
        }

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
                thread::sleep(Duration::from_millis(50));
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
    buffer.clear();
    match reader.read_until(b'\0', buffer) {
        Ok(0) => Err("Client disconnected".into()),
        Ok(n) => {
            // Only process if we actually got data
            if n > 0 {
                buffer.pop(); // Remove null terminator
                let request_str = parse_request(buffer)?;
                let request: DatabaseRequest = serde_json::from_str(&request_str)
                    .map_err(|e| format!("Failed to parse request: {}", e))?;
                parse_command_from_request(conn, request, stream, client_id, clients)?;
            }
            Ok(())
        }
        Err(e) if e.kind() == WouldBlock => {
            // No data available - this is expected in non-blocking mode
            Err("WouldBlock".into())
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
            let conn = conn.lock().unwrap();
            handle_database_request(&conn, request, stream)
        }
        "UPDATE" => {
            let conn = conn.lock().unwrap();
            handle_database_request(&conn, request, stream)
        }
        "DELETE" => {
            let conn = conn.lock().unwrap();
            handle_database_request(&conn, request, stream)
        }
        "KEY_EXCHANGE" => {
            let success = key_exchange();
            let reply = if success {
                DatabaseReply::success(0)
            } else {
                DatabaseReply::error()
            };
            match send_response(&reply, stream) {
                Ok(_) => Ok(()),
                Err(e) => {
                    eprintln!("Error with sending back to checkpoint: {}", e);
                    Err(e)
                }
            }
        }
        _ => Err("Unknown command".into()),
    }
}

/*
 * Name: handle_init_request
 * Function: Handles checkpoint initialization requests by:
 * 1. Checking if the checkpoint already exists in local database
 * 2. If not, adding it with location and allowed roles
 * 3. Querying central database for checkpoint ID
 * 4. Returning the checkpoint ID to the requesting checkpoint
 */
fn handle_init_request(
    conn: &Arc<Mutex<Connection>>,
    request: DatabaseRequest,
    stream: &Arc<Mutex<TcpStream>>,
) -> Result<(), String> {
    println!("Received INIT request from checkpoint");

    // Get required fields from request
    let location = request
        .location
        .clone()
        .ok_or("Location is missing in request")?;
    let allowed_roles = request
        .authorized_roles
        .clone()
        .ok_or("Allowed roles are missing in request")?;

    // Query central database first (without holding the lock)
    let db_reply = query_database(DATABASE_ADDR, &request)
        .map_err(|e| format!("Database query failed: {}", e))?;

    if db_reply.status != "success" {
        println!("Central database returned an error for INIT request");
        return send_response(&DatabaseReply::error(), stream);
    }

    let checkpoint_id = db_reply
        .checkpoint_id
        .ok_or("Central database didn't return a checkpoint ID")?;

    println!(
        "Received checkpoint ID {} from central database",
        checkpoint_id
    );

    // Now lock the connection for local DB operations
    let conn = conn.lock().unwrap();

    // Check if checkpoint exists
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM checkpoints WHERE id = ?)",
            params![checkpoint_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to query checkpoint existence: {}", e))?;

    if exists {
        println!("Checkpoint '{}' already exists in local database", location);
    } else {
        // Insert new checkpoint
        conn.execute(
            "INSERT INTO checkpoints (id, location, allowed_roles) VALUES (?1, ?2, ?3)",
            params![checkpoint_id, location, allowed_roles],
        )
        .map_err(|e| format!("Failed to insert checkpoint: {}", e))?;

        println!("Added new checkpoint '{}' to local database", checkpoint_id);
    }

    // Send success response
    send_response(&DatabaseReply::init_reply(checkpoint_id), stream)
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

    let worker_id = request
        .worker_id
        .ok_or("Worker ID is missing".to_string())?;
    println!("Worker ID is {}", worker_id);

    let response = match client.state {
        CheckpointState::WaitForRfid => {
            if authenticate_rfid(
                &conn.lock().unwrap(),
                &Some(worker_id),
                &request.checkpoint_id,
            ) {
                println!("RFID Verified: {:?} matches database entry.", worker_id);
                println!("Next state: WaitForFingerprint");

                client.state = CheckpointState::WaitForFingerprint;
                CheckpointReply {
                    status: "success".to_string(),
                    checkpoint_id: request.checkpoint_id.map(|id| id.into()),
                    worker_id: Some(worker_id),
                    fingerprint: None,
                    data: None,
                    auth_response: Some(CheckpointState::WaitForFingerprint),
                    rfid_ver: Some(true),
                }
            } else {
                println!("Next state: AuthFailed");

                client.state = CheckpointState::AuthFailed;
                CheckpointReply {
                    status: "failed".to_string(),
                    checkpoint_id: request.checkpoint_id.map(|id| id.into()),
                    worker_id: None,
                    fingerprint: None,
                    data: None,
                    auth_response: Some(CheckpointState::AuthFailed),
                    rfid_ver: Some(false),
                }
            }
        }
        CheckpointState::WaitForFingerprint => {
            if authenticate_fingerprint(
                &conn.lock().unwrap(),
                &Some(worker_id),
                &request.worker_fingerprint,
                &request.checkpoint_id,
            ) {
                println!("Next state: AuthSuccessful");

                client.state = CheckpointState::AuthSuccessful;
                CheckpointReply::auth_reply(CheckpointState::AuthSuccessful)
            } else {
                println!("Next state: AuthFailed");

                client.state = CheckpointState::AuthFailed;
                CheckpointReply::auth_reply(CheckpointState::AuthFailed)
            }
        }
        _ => {
            return Err("Invalid state".to_string());
        }
    };

    if client.state == CheckpointState::AuthSuccessful
        || client.state == CheckpointState::AuthFailed
    {
        println!("Next state: WaitForRfid");

        send_response(&CheckpointReply::auth_reply(client.state.clone()), stream).map_err(|e| {
            eprintln!("Failed to send response back to checkpoint: {}", e);
            e
        })?;
        thread::sleep(Duration::from_secs(5));
        client.state = CheckpointState::WaitForRfid;
    } else {
        send_response(&response, stream).map_err(|e| {
            eprintln!("Failed to send response back to checkpoint: {}", e);
            e
        })?;
    }

    Ok(())
}

/* Name: handle_database_request
 * Function: handles Update, Enroll and Delete requests from the centralized database.
 */
fn handle_database_request(
    conn: &Connection,
    request: DatabaseRequest,
    stream: &Arc<Mutex<TcpStream>>,
) -> Result<(), String> {
    let db_reply = query_database(DATABASE_ADDR, &request)
        .map_err(|e| format!("Database query failed: {}", e))?;

    let reply = if db_reply.status == "success" {
        match request.command.as_str() {
            "ENROLL" => DatabaseReply::success(db_reply.worker_id.unwrap()),
            "DELETE" => {
                let worker_id = request
                    .worker_id
                    .ok_or("Worker ID is missing in the request")?;

                if check_local_db(conn, worker_id).map_err(|e| format!("Database error: {}", e))? {
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
                    eprintln!(
                        "Worker with ID {} not found in the local database",
                        worker_id
                    );
                    DatabaseReply::error()
                }
            }
            "UPDATE" => {
                let worker_id = request
                    .worker_id
                    .ok_or("Worker ID is missing in the request")?;
                request.role_id.ok_or("Role ID is missing in the request")?;
                request
                    .location
                    .ok_or("Allowed locations are missing in the request")?;

                if check_local_db(conn, worker_id).map_err(|e| format!("Database error: {}", e))? {
                    match update_worker_entry(
                        conn,
                        worker_id,
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
        DatabaseReply::error()
    };

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
fn log_event(worker_id: Option<u64>, checkpoint_id: Option<u32>, method: &str, status: &str) {
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

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();

        conn.execute(
            "CREATE TABLE roles (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL
            )",
            [],
        )
        .unwrap();

        conn.execute(
            "CREATE TABLE employees (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                fingerprint_id INTEGER NOT NULL,
                role_id INTEGER NOT NULL,
                allowed_locations TEXT NOT NULL,
                rfid_data INTEGER NOT NULL,
                FOREIGN KEY (role_id) REFERENCES roles (id)
            )",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO roles (id, name) VALUES 
            (1, 'Admin'), (2, 'Worker')",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO employees 
             (id, name, fingerprint_id, role_id, allowed_locations, rfid_data) VALUES 
             (1, 'Admin User', 12345, 1, 'Gate1,Gate2', 1001),
             (2, 'Regular Worker', 67890, 2, 'Gate2', 1002)",
            [],
        )
        .unwrap();

        conn
    }

    #[test]
    fn test_fingerprint_auth() {
        let conn = setup_test_db();

        // Valid fingerprint
        assert!(authenticate_fingerprint(
            &conn,
            &Some(1),
            &Some("12345".to_string()),
            &Some(1)
        ));

        // Invalid fingerprint
        assert!(!authenticate_fingerprint(
            &conn,
            &Some(1),
            &Some("99999".to_string()),
            &Some(1)
        ));

        // Non-existent user
        assert!(!authenticate_fingerprint(
            &conn,
            &Some(999),
            &Some("12345".to_string()),
            &Some(1)
        ));
    }

    #[test]
    fn test_add_worker() {
        let conn = setup_test_db();

        assert!(add_to_local_db(
            &conn,
            3,
            "New Worker".to_string(),
            54321,
            2,
            "Gate1".to_string(),
            1003
        )
        .is_ok());

        assert!(check_local_db(&conn, 3).unwrap());
    }

    #[test]
    fn test_rfid_auth() {
        let conn = setup_test_db();

        // Setup checkpoints table
        conn.execute(
            "CREATE TABLE checkpoints (
                id INTEGER PRIMARY KEY,
                location TEXT NOT NULL,
                allowed_roles TEXT NOT NULL
            )",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO checkpoints (id, location, allowed_roles) VALUES
            (1, 'Gate1', 'Admin,Worker'),
            (2, 'Gate2', 'Admin')",
            [],
        )
        .unwrap();

        // Admin can access Gate1
        assert!(authenticate_rfid(&conn, &Some(1), &Some(1)));

        // Worker cannot access Gate2 (admin only)
        assert!(!authenticate_rfid(&conn, &Some(2), &Some(2)));
    }
}
