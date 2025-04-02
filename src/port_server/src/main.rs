/****************
    IMPORTS
****************/
use chrono::Local;
use common::{
    CheckpointReply, CheckpointState, Client, DatabaseReply, DatabaseRequest, Role, DATABASE_ADDR,
    SERVER_ADDR,Parameters, keygen_string, 
    decrypt_string, encrypt_aes
};
use rusqlite::{params, Connection, Result};
use std::fs::OpenOptions;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write, ErrorKind::WouldBlock},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use lazy_static::lazy_static;
use serde_json::{json, Value};
use base64::{engine::general_purpose, Engine as _};

const LOG_FILE: &str = "auth.log";

/*
* Name: Lazy Static 
* Function: For generating and storing a server keypair, also provides static reference for AES key and IV
*/
lazy_static! {
    // PS_KEYPAIR will hold the port server's persistent key pair.
    // keygen_string returns a HashMap with keys "public" and "secret".
    static ref PS_KEYPAIR: Mutex<HashMap<String, String>> = Mutex::new({
        let params = Parameters::default();
        let keypair = keygen_string(&params, None);
        println!("Port Server Public Key: {}", keypair.get("public").unwrap()); // For debugging
        keypair
    });
}
lazy_static! {
    static ref SYMMETRIC_KEY: Mutex<Option<String>> = Mutex::new(None);
    static ref SYMMETRIC_IV: Mutex<Option<String>> = Mutex::new(None);
}

/**
 * Name: initialize_database
 * Function: Initializes a local employees database table for authentication.
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
            fingerprint_ids INTEGER NOT NULL,
            role_id INTEGER NOT NULL,
            allowed_locations TEXT NOT NULL,
            rfid_data TEXT NOT NULL,
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

/*
 * Name: check_local_db
 * Function: checks if a worker is in the local database.
 */
fn check_local_db(conn: &Connection, id: u64) -> Result<bool> {
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
    id: u64,
    name: String,
    fingerprint_json: String,
    role_id: i32,
    allowed_locations: String,
    rfid_data: u32,
) -> Result<(), rusqlite::Error> {
    fn add_to_local_db_inner(
        conn: &Connection,
        id: u64,
        name: String,
        new_fingerprint_json: Value,
        role_id: i32,
        allowed_locations: String,
        rfid_data: u32,
    ) -> Result<(), rusqlite::Error> {
        let existing_json: Option<String> = conn.query_row(
            "SELECT fingerprint_ids FROM employees WHERE id = ?1",
            params![id],
            |row| row.get(0),
        ).ok();

        let mut fingerprint_data: Value = existing_json
            .as_ref()
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_else(|| json!({ "fingerprints": {} })); 


        conn.execute(
            "INSERT OR REPLACE INTO employees (id, name, fingerprint_ids, role_id, allowed_locations, rfid_data) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, name, serde_json::to_string(&fingerprint_data).unwrap(), role_id, allowed_locations, rfid_data],  
        )?;

        Ok(())
    }

    let fingerprint_value: Value = serde_json::from_str(&fingerprint_json)
        .map_err(|_e| rusqlite::Error::InvalidQuery)?;
    
    add_to_local_db_inner(conn, id, name, fingerprint_value, role_id, allowed_locations, rfid_data)
}

/*
 * Name: delete_from_local_db
 * Function: deletes a worker from the port server's database.
 */
fn delete_from_local_db(conn: &Connection, id: u64) -> Result<(), rusqlite::Error> {
    conn.execute("DELETE FROM employees WHERE id = ?1", params![id])?;
    Ok(())
}

/*
 * Name: update_worker_entry
 * Function: updates a worker's information in the local database.
 */
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
                    log_event(rfid_tag.map(|id| id.into()), checkpoint_id.map(|id| id.into()), "RFID", "Failed");
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
                    log_event(rfid_tag.map(|id| id.into()), checkpoint_id.map(|id| id.into()), "RFID", "Failed");
                    return false;
                }
            };

            let allowed_roles: String = match stmt.query_row([checkpoint], |row| row.get(0)) {
                Ok(roles) => roles,
                Err(e) => {
                    eprintln!("Role query failed: {}", e);
                    log_event(rfid_tag.map(|id| id.into()), checkpoint_id.map(|id| id.into()), "RFID", "Failed");
                    return false;
                }
            };

            let allowed_roles_vec: Vec<String> = allowed_roles
                .split(',')
                .map(|role| role.trim().to_string())
                .collect();

            if !allowed_roles_vec.contains(&role_name) {
                println!("User does not have the required role");
                log_event(rfid_tag.map(|id| id.into()), checkpoint_id.map(|id| id.into()), "RFID", "Failed");
                return false;
            } else {
                log_event(rfid_tag.map(|id| id.into()), checkpoint_id.map(|id| id.into()), "RFID", "Successful");
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
                    log_event(Some(u64::from(*rfid)), Some(u32::from(*checkpoint)), "RFID", "Failed");
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

                   let auth_successful = Some(u64::from(*rfid)) == response.worker_id.map(|id| u64::from(id)) && authorized_roles.contains(&role_str)
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
                    log_event(Some(u64::from(*rfid)), Some(u32::from(*checkpoint)), "RFID", "Successful");
                } else {
                    log_event(Some(u64::from(*rfid)), Some(u32::from(*checkpoint)), "RFID", "Failed");
                }

                return auth_successful;
            }
            Err(e) => {
                eprintln!("Error querying database for RFID: {:?}", e);
                log_event(rfid_tag.map(|id| id.into()), checkpoint_id.map(|id| id.into()), "RFID", "Failed");
                return false;
            }
        }
    } else {
        log_event(rfid_tag.map(|id| id.into()), checkpoint_id.map(|id| id.into()), "RFID", "Failed");
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
    fingerprint_ids: &Option<String>,
    checkpoint_id: &Option<u32>,
) -> bool {
    // Early return if any required field is missing
    let (rfid, fingerprint, checkpoint) = match (rfid_tag, fingerprint_ids, checkpoint_id) {
        (Some(r), Some(f), Some(c)) => (r, f, c),
        _ => {
            log_event(
                rfid_tag.map(|id| id.into()),
                checkpoint_id.map(|id| id.into()),
                "Fingerprint",
                "Failed - Missing data"
            );
            return false;
        }
    };

    // Check local database first
    if check_local_db(conn, *rfid).unwrap_or(false) {
        println!("Found worker in local database");
        let mut stmt = match conn.prepare(
            "SELECT fingerprint_ids
             FROM employees
             WHERE employees.id = ?",
        ) {
            Ok(stmt) => stmt,
            Err(e) => {
                eprintln!("STMT failed: {}", e);
                log_event(Some(*rfid), Some(*checkpoint), "Fingerprint", "Failed");
                return false;
            }
        };

        let fp_id: u32 = match stmt.query_row([rfid], |row| row.get(0)) {
            Ok(fp_id) => fp_id,
            Err(e) => {
                eprintln!("Query failed: {}", e);
                return false;
            }
        };
        let auth_successful = fp_id.to_string() == *fingerprint;
        println!("Comparing {} to {:?}", fp_id.to_string(), *fingerprint);
        if auth_successful {
            log_event(Some(*rfid), Some(*checkpoint), "Fingerprint", "Success");
            return true;
        } else {
            log_event(Some(*rfid), Some(*checkpoint), "Fingerprint", "Failed");
            return false;
        }
    }

    // If not found locally, query central database
    let request = DatabaseRequest {
        command: "AUTHENTICATE".to_string(),
        checkpoint_id: Some(*checkpoint),
        worker_id: Some(*rfid),
        rfid_data: None,
        worker_fingerprint: Some(fingerprint.clone()),
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
            // Verify the response data
            let auth = match (response.worker_id, response.worker_fingerprint) {
                (Some(db_rfid), Some(db_fingerprint)) => {
                    *rfid == db_rfid && *fingerprint == db_fingerprint.to_string()
                }
                _ => false,
            };

            if auth {
                // Add to local cache if authentication succeeds
                if let (Some(id), Some(name), Some(fp), Some(role), Some(locations), Some(rfid_data)) = (
                    response.worker_id,
                    response.worker_name,
                    response.worker_fingerprint,
                    response.role_id,
                    response.allowed_locations,
                    response.rfid_data,
                ) {
                    if let Err(e) = add_to_local_db(
                        conn,
                        id,
                        name,
                        fp.to_string(),
                        role as i32,
                        locations,
                        rfid_data,
                    ) {
                        eprintln!("Failed to add to local DB: {}", e);
                    }
                }
                log_event(Some(*rfid), Some(*checkpoint), "Fingerprint", "Successful");
                true
            } else {
                log_event(Some(*rfid), Some(*checkpoint), "Fingerprint", "Failed - Mismatch");
                false
            }
        }
        Ok(_) => {
            log_event(Some(*rfid), Some(*checkpoint), "Fingerprint", "Failed - DB error");
            false
        }
        Err(e) => {
            eprintln!("Database query error: {}", e);
            log_event(Some(*rfid), Some(*checkpoint), "Fingerprint", "Failed - Connection error");
            false
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

    let encrypted_request = if aes_key_opt.is_some() && aes_iv_opt.is_some() && request.command != "KEY_EXCHANGE" {
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
    let my_public_key = ps_keypair.get("public").expect("Public key not found").clone();
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
                    let decrypted_iv =
                        decrypt_string(my_private_key, &encrypted_iv, &rlwe_params);
                    
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
                Ok(_) => {
                    Ok(())
                }
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
    let location = request.location.clone().ok_or("Location is missing in request")?;
    let allowed_roles = request.authorized_roles.clone().ok_or("Allowed roles are missing in request")?;
    
    let conn = conn.lock().unwrap();
    
    // First check if checkpoint already exists in local DB
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM checkpoints WHERE location = ?)",
        params![location],
        |row| row.get(0),
    ).map_err(|e| format!("Failed to check checkpoint existence: {}", e))?;
    
    if exists {
        println!("Checkpoint '{}' already exists in local database", location);
    } else {
        // Insert new checkpoint
        conn.execute(
            "INSERT INTO checkpoints (location, allowed_roles) VALUES (?1, ?2)",
            params![location, allowed_roles],
        ).map_err(|e| format!("Failed to insert checkpoint: {}", e))?;
        
        println!("Added new checkpoint '{}' to local database", location);
    }
    
    // Query central database to get/set checkpoint ID
    let reply = query_database(DATABASE_ADDR, &request)
        .map(|db_reply| {
            if db_reply.status == "success" {
                let checkpoint_id = db_reply.checkpoint_id.unwrap();
                println!("Received checkpoint ID {} from central database", checkpoint_id);
                
                // Update local DB with the ID if it was assigned by central DB
                if let Err(e) = conn.execute(
                    "UPDATE checkpoints SET id = ?1 WHERE location = ?2",
                    params![checkpoint_id, location],
                ) {
                    eprintln!("Warning: Failed to update checkpoint ID in local DB: {}", e);
                }
                
                DatabaseReply::init_reply(checkpoint_id)
            } else {
                println!("Central database returned an error for INIT request");
                DatabaseReply::error()
            }
        })
        .map_err(|e| format!("Database query failed: {}", e))?;
    
    // Send response back to checkpoint
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

    let worker_id = request.worker_id.ok_or("Worker ID is missing".to_string())?;
    println!("Worker ID is {}", worker_id);

    let response = match client.state {
        CheckpointState::WaitForRfid => {
            if authenticate_rfid(&conn.lock().unwrap(), &Some(worker_id), &request.checkpoint_id) {
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

    if client.state == CheckpointState::AuthSuccessful || client.state == CheckpointState::AuthFailed {
        println!("Next state: WaitForRfid");

        send_response(&CheckpointReply::auth_reply(client.state.clone()), stream)
        .map_err(|e| {
            eprintln!("Failed to send response back to checkpoint: {}", e);
            e
        })?;
        thread::sleep(Duration::from_secs(5));
        client.state = CheckpointState::WaitForRfid;
    } else {
        send_response(&response, stream)
        .map_err(|e| {
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

    fn setup_test_database() -> Connection {
        let conn = Connection::open(":memory:").expect("Failed to create in-memory database");

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
                fingerprint_data JSON NOT NULL,
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

        conn.execute(
            "INSERT INTO roles (id, name) VALUES (1, 'Admin'), (2, 'Worker')",
            [],
        )
        .expect("Failed to insert roles");

        conn.execute(
            "INSERT INTO employees (id, name, fingerprint_data, role_id, allowed_locations) VALUES 
            (1, 'John Doe', '{\"fingerprints\":{\"1\":123}}', 1, 'Location1,Location2'),
            (2, 'Jane Doe', '{\"fingerprints\":{\"2\":456}}', 2, 'Location2')",
            [],
        )
        .expect("Failed to insert employees");

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

        let exists = check_local_db(&conn, 1).expect("Failed to check local database");
        assert!(exists);

        let exists = check_local_db(&conn, 999).expect("Failed to check local database");
        assert!(!exists);
    }

    #[test]
    fn test_add_to_local_db() {
        let conn = setup_test_database();

        add_to_local_db(
            &conn,
            3,
            "New Employee".to_string(),
            "{\"fingerprints\":{\"1\":789}}".to_string(),
            2,
            "Location2".to_string(),
        )
        .expect("Failed to add to local database");

        let exists = check_local_db(&conn, 3).expect("Failed to check local database");
        assert!(exists);
    }

    #[test]
    fn test_delete_from_local_db() {
        let conn = setup_test_database();

        delete_from_local_db(&conn, 1).expect("Failed to delete from local database");

        let exists = check_local_db(&conn, 1).expect("Failed to check local database");
        assert!(!exists);
    }

    #[test]
    fn test_update_worker_entry() {
        let conn = setup_test_database();

        update_worker_entry(&conn, 1, "Location3".to_string(), 2)
            .expect("Failed to update worker entry");

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

        let mock_tag: Option<u32> = Some(1);
        let mock_checkpoint: Option<u32> = Some(1);
        let result = authenticate_rfid(&conn, &mock_tag, &mock_checkpoint);
        assert!(result);

        let mock_tag_mismatch_role: Option<u32> = Some(2);
        let result_mismatch_role =
            authenticate_rfid(&conn, &mock_tag_mismatch_role, &mock_checkpoint);
        assert!(!result_mismatch_role);

        let mock_tag_invalid: Option<u32> = Some(999);
        let result_invalid = authenticate_rfid(&conn, &mock_tag_invalid, &mock_checkpoint);
        assert!(!result_invalid);
    }

    #[test]
    fn test_authenticate_fingerprint() {
        let conn = setup_test_database();

        let mock_tag: Option<u32> = Some(1);
        let mock_fingerprint: Option<String> = Some("123".to_string());
        let mock_checkpoint: Option<u32> = Some(1);
        let result =
            authenticate_fingerprint(&conn, &mock_tag, &mock_fingerprint, &mock_checkpoint);
        assert!(result);

        let mock_fingerprint_invalid: Option<String> = Some("wrong_hash".to_string());
        let result_invalid = authenticate_fingerprint(
            &conn,
            &mock_tag,
            &mock_fingerprint_invalid,
            &mock_checkpoint,
        );
        assert!(!result_invalid);

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
