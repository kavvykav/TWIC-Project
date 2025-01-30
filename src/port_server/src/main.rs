/****************
    IMPORTS
****************/
use common::{
    CheckpointReply, CheckpointState, Client, DatabaseReply, DatabaseRequest, Role, DATABASE_ADDR,
    SERVER_ADDR,
};
use ctrlc;
use rusqlite::{params, Connection, Error, Result};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

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
                Err(_) => return false,
            };

            let role_name: String = match stmt.query_row([rfid], |row| row.get(0)) {
                Ok(role) => role,
                Err(_) => return false,
            };

            let mut stmt = match conn.prepare(
                "SELECT allowed_roles
                 FROM checkpoints
                 WHERE id = ?",
            ) {
                Ok(stmt) => stmt,
                Err(_) => return false,
            };

            let allowed_roles: String = match stmt.query_row([checkpoint], |row| row.get(0)) {
                Ok(roles) => roles,
                Err(_) => return false,
            };

            let allowed_roles_vec: Vec<String> = allowed_roles
                .split(',')
                .map(|role| role.trim().to_string())
                .collect();

            if !allowed_roles_vec.contains(&role_name) {
                return false;
            } else {
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
                    rfid, response.worker_id
                );
                println!("Response status: {}", response.status);

                if response.status != "success".to_string() {
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

                return Some(rfid) == response.worker_id.as_ref()
                    && authorized_roles.contains(&role_str)
                    && allowed_locations_vec.contains(&response.location.unwrap());
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
                Err(_) => return false,
            };

            let fingerprint_hash: String = match stmt.query_row([rfid], |row| row.get(0)) {
                Ok(fp) => fp,
                Err(_) => return false,
            };

            // Check if the hashes are equal
            return fingerprint == &fingerprint_hash;
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
        if let Err(e) = read_request(
            &conn,
            &mut reader,
            &stream,
            client_id,
            &clients,
            &mut buffer,
        ) {
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
    conn: &Arc<Mutex<Connection>>,
    request: DatabaseRequest,
    stream: &Arc<Mutex<TcpStream>>,
) -> Result<(), String> {
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
                DatabaseReply::init_reply(db_reply.checkpoint_id.unwrap())
            } else {
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

    let response = match client.state {
        CheckpointState::WaitForRfid => {
            if authenticate_rfid(
                &conn.lock().unwrap(),
                &request.worker_id,
                &request.checkpoint_id,
            ) {
                client.state = CheckpointState::WaitForFingerprint;
                CheckpointReply::auth_reply(CheckpointState::WaitForFingerprint)
            } else {
                client.state = CheckpointState::AuthFailed;
                CheckpointReply::auth_reply(CheckpointState::AuthFailed)
            }
        }
        CheckpointState::WaitForFingerprint => {
            if authenticate_fingerprint(
                &conn.lock().unwrap(),
                &request.worker_id,
                &request.worker_fingerprint,
                &request.checkpoint_id,
            ) {
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
 * Function: handles Update, Enroll and Delete requests from the centralized database.
 */
fn handle_database_request(
    request: DatabaseRequest,
    stream: &Arc<Mutex<TcpStream>>,
) -> Result<(), String> {
    let reply = query_database(DATABASE_ADDR, &request)
        .map(|db_reply| {
            if db_reply.status == "success" {
                DatabaseReply::init_reply(request.checkpoint_id.unwrap())
            } else {
                DatabaseReply::error()
            }
        })
        .map_err(|e| format!("Database query failed: {}", e))?;
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

// Main server function
fn main() -> Result<(), rusqlite::Error> {
    let listener = TcpListener::bind(SERVER_ADDR).expect("Failed to bind address");
    listener
        .set_nonblocking(true)
        .expect("Cannot set non-blocking mode");
    println!("Server listening on {}", SERVER_ADDR);

    let clients: Arc<Mutex<HashMap<usize, Client>>> = Arc::new(Mutex::new(HashMap::new()));
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = Arc::clone(&running);

    // Database Initialization
    let database = initialize_database()?;
    let database = Arc::new(Mutex::new(database));

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
    fn test_authenticate_rfid() {
        let conn = setup_test_database();

        // Test valid RFID and checkpoint
        let mock_tag: Option<u32> = Some(1);
        let mock_checkpoint: Option<u32> = Some(1);
        let result = authenticate_rfid(&conn, &mock_tag, &mock_checkpoint);
        assert_eq!(result, true);

        // Test invalid RFID (wrong role for checkpoint)
        let mock_tag_mismatch_role: Option<u32> = Some(2);
        let result_mismatch_role =
            authenticate_rfid(&conn, &mock_tag_mismatch_role, &mock_checkpoint);
        assert_eq!(result_mismatch_role, false);

        // Test invalid RFID (non-existent)
        let mock_tag_invalid: Option<u32> = Some(999);
        let result_invalid = authenticate_rfid(&conn, &mock_tag_invalid, &mock_checkpoint);
        assert_eq!(result_invalid, false);
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
        assert_eq!(result, true);

        // Test invalid fingerprint
        let mock_fingerprint_invalid: Option<String> = Some("wrong_hash".to_string());
        let result_invalid = authenticate_fingerprint(
            &conn,
            &mock_tag,
            &mock_fingerprint_invalid,
            &mock_checkpoint,
        );
        assert_eq!(result_invalid, false);

        // Test invalid RFID
        let mock_tag_invalid: Option<u32> = Some(999);
        let result_invalid_rfid = authenticate_fingerprint(
            &conn,
            &mock_tag_invalid,
            &mock_fingerprint,
            &mock_checkpoint,
        );
        assert_eq!(result_invalid_rfid, false);
    }
}
