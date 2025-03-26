/****************
    IMPORTS
****************/
use common::{DatabaseReply, DatabaseRequest, Role, DATABASE_ADDR, Parameters, 
    keygen_string, encrypt_string, decrypt_string, encrypt_aes, decrypt_aes, generate_iv, generate_key
};
use rusqlite::{params, Connection, Result};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use lazy_static::lazy_static;
use std::sync::Mutex as StdMutex;
use std::fs::File;
use std::io::Write;


/*
* Name: Lazy Static 
* Function: For generating and storing a server keypair, also provides static reference for AES key and IV
*/
lazy_static! {
    static ref DB_KEYPAIR: std::sync::Mutex<(String, String)> = std::sync::Mutex::new({
        let params = Parameters::default();
        let keypair = keygen_string(&params, None);
        (
            keypair.get("public").expect("Public key not found").to_string(),
            keypair.get("secret").expect("Private key not found").to_string()
        )
    });
}

lazy_static! {
    // Global variables to hold the symmetric AES key and IV as hex strings.
    static ref AES_KEY: StdMutex<Option<String>> = StdMutex::new(None);
    static ref IV: StdMutex<Option<String>> = StdMutex::new(None);
}


/*
* Name: write_db_public_key
* Function: Saves public key for usage 
*/
fn write_db_public_key() {
    let keypair = DB_KEYPAIR.lock().unwrap();
    let public_key = &keypair.0;
    let mut file = File::create("db_public_key.txt")
        .expect("Unable to create public key file");
    file.write_all(public_key.as_bytes())
        .expect("Unable to write public key");
}






/*
* Name: initialize_database
* Function: initializes the centralized database by creating all the tables,
*           returns a connection to the database.
*/
fn initialize_database() -> Result<Connection> {
    let conn = Connection::open("system.db")?;

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
            fingerprint_ids TEXT NOT NULL,
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
        "INSERT OR IGNORE INTO checkpoints (id, location, allowed_roles) VALUES 
        (999, 'AdminSystem', 'Admin')",
        [],
    )?;

    Ok(conn)
}

/*
* Name: handle_port_server_request
* Function: Searches for the command in the Request structure from the port server,
*           and services the request accordingly.
*/
async fn handle_port_server_request(
    conn: Arc<Mutex<Connection>>,
    req: DatabaseRequest,
) -> DatabaseReply {
    let conn = conn.lock().await;
    println!("Received a command: {}", req.command);
    let rlwe_params = Parameters::default();

    match req.command.as_str() {
        "INIT_REQUEST" => {
            let result = conn.execute(
                "INSERT INTO checkpoints (location, allowed_roles) VALUES (?1, ?2)",
                params![req.location, req.authorized_roles],
            );
            match result {
                Ok(_) => {
                    let checkpoint_id = conn.last_insert_rowid() as u32;
                    println!("Added checkpoint to the database! ID is {}", checkpoint_id);
                    return DatabaseReply::init_reply(checkpoint_id);
                }
                Err(e) => {
                    eprintln!("Issue with adding checkpoint to the database: {}", e);
                    return DatabaseReply::error();
                }
            }
        }

        "AUTHENTICATE" => {
            // Checkpoint details
            println!(
                "Checkpoint id is: {}",
                req.checkpoint_id.unwrap_or_default()
            );

            // If employee does not exist send back an error
            if !employee_exists(&conn, req.worker_id.unwrap()).unwrap() {
                println!("Worker des not exist");
                return DatabaseReply::error();
            }

            // Fetch checkpoint data
            let checkpoint_data: Result<(String, String), _> = conn.query_row(
                "SELECT location, allowed_roles FROM checkpoints WHERE id = ?1",
                params![req.checkpoint_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            );

            match checkpoint_data {
                Ok((location, allowed_roles)) => {
                    // Worker details
                    let worker_data: Result<(String, String, String, u32), _> = conn.query_row(
                "SELECT employees.fingerprint_hash, employees.allowed_locations, employees.name, roles.id \
                 FROM employees \
                 JOIN roles ON employees.role_id = roles.id \
                 WHERE employees.id = ?1",
                params![req.worker_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            );

                    match worker_data {
                        Ok((worker_fingerprint, allowed_locations, name, role_id)) => {
                            // Return the authentication reply
                            return DatabaseReply::auth_reply(
                                req.checkpoint_id.unwrap_or_default(),
                                req.worker_id.unwrap_or_default(),
                                worker_fingerprint,
                                role_id,
                                allowed_roles,
                                location,
                                allowed_locations,
                                name,
                            );
                        }
                        Err(e) => {
                            // Error fetching worker details
                            eprintln!("Error fetching worker details: {}", e);
                            return DatabaseReply::error();
                        }
                    }
                }
                Err(e) => {
                    // Error fetching checkpoint details
                    eprintln!("Error fetching checkpoint details: {}", e);
                    return DatabaseReply::error();
                }
            }
        }
        "ENROLL" => {
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM employees WHERE name = ?1 AND role_id = ?2)",
                    params![req.worker_name, req.role_id],
                    |row| row.get(0),
                )
                .unwrap_or(false);

            if exists {
                println!("User already exists!");
                return DatabaseReply::error();
            }

            let result = conn.execute(
                "INSERT INTO employees (name, fingerprint_hash, role_id, allowed_locations) VALUES (?1, ?2, ?3, ?4)",
                params![req.worker_name, req.worker_fingerprint, req.role_id, req.location],
            );
            // fetch id
            let latest_id: i64 = conn
                .query_row("SELECT LAST_INSERT_ROWID()", [], |row| row.get(0))
                .unwrap();
            let worker_id = latest_id as u32;
            match result {
                Ok(id) => {
                    return DatabaseReply::success(worker_id);
                }

                Err(e) => {
                    eprintln!("Could not enroll user {}", e);
                    return DatabaseReply::error();
                }
            }
        }

        "UPDATE" => {
            let result = conn.execute(
                "UPDATE employees SET role_id = ?1, allowed_locations = ?2 WHERE id = ?3",
                params![req.role_id, req.location, req.worker_id],
            );
            match result {
                Ok(affected) => {
                    if affected > 0 {
                        return DatabaseReply::update_success(
                            req.location.unwrap(),
                            req.role_id.unwrap(),
                        );
                    } else {
                        println!("Zero affected users");
                        return DatabaseReply::error();
                    }
                }
                Err(e) => {
                    eprintln!("An error occured with adding a user: {}", e);
                    return DatabaseReply::error();
                }
            }
        }
        "DELETE" => {
            let result = conn.execute(
                "DELETE FROM employees WHERE id = ?1",
                params![req.worker_id],
            );
            match result {
                Ok(affected) => {
                    if affected > 0 {
                        return DatabaseReply::success(0);
                    } else {
                        println!("Affected users is zero");
                        return DatabaseReply::error();
                    }
                }
                Err(e) => {
                    eprintln!("Error with deleting a worker: {}", e);
                    return DatabaseReply::error();
                }
            }
        }
        "KEY_EXCHANGE" => {
            let port_public_key = req.public_key.as_ref().expect("Missing port server public key");
            let aes_key_temp = generate_key();
            let iv_temp = generate_iv();

            let encrypted_aes_key = encrypt_string(port_public_key, &aes_key_temp, &rlwe_params, None);
            let encrypted_iv = encrypt_string(port_public_key, &iv_temp, &rlwe_params, None);

            println!("Database generated AES Key: {:?}", aes_key_temp);
            println!("Database generated IV: {:?}", iv_temp);

            AES_KEY.lock().unwrap().replace(hex::encode(&aes_key_temp));
            IV.lock().unwrap().replace(hex::encode(&iv_temp));
            return DatabaseReply {
                status: "success".to_string(),
                checkpoint_id: None,
                worker_id: None,
                worker_fingerprint: None,
                role_id: None,
                authorized_roles: None,
                location: None,
                auth_response: None,
                allowed_locations: None,
                worker_name: None,
                encrypted_aes_key: Some(encrypted_aes_key),
                encrypted_iv: Some(encrypted_iv),
                public_key: None,
            };
        }
        _ => {
            println!("Unknown command");
            return DatabaseReply::error();
        }
    }
}

/*
 * Name: employee_exists
 * Function: Check if employee exists in the database.
 */
fn employee_exists(conn: &Connection, id: u32) -> Result<bool> {
    let mut stmt = conn.prepare("SELECT 1 FROM employees WHERE id = ? LIMIT 1")?;
    let mut rows = stmt.query(params![id])?;
    Ok(rows.next()?.is_some())
}

/*
* Name: main
* Function: Main program for the database node, opens a socket and services oncoming
*           TCP connections.
*/
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database = initialize_database()?;
    let database = Arc::new(Mutex::new(database));

    write_db_public_key();

    let listener = TcpListener::bind(DATABASE_ADDR).await?;
    println!("Database server is listening on {}", DATABASE_ADDR);

    loop {
        let (mut socket, addr) = listener.accept().await?;
        println!("Accepted connection from {}", addr);

        let database = Arc::clone(&database);

        tokio::spawn(async move {
            let mut buffer = vec![0; 9000];

            match socket.read(&mut buffer).await {
                Ok(0) => println!("Client at {} has closed the connection", addr),
                Ok(n) => {
                    let request_json = String::from_utf8_lossy(&buffer[..n]);
                    let request: Result<DatabaseRequest, _> = serde_json::from_str(&request_json);

                    let database_reply = match request {
                        Ok(req) => handle_port_server_request(database, req).await,
                        Err(_) => DatabaseReply::error(),
                    };

                    let mut reply_json = match serde_json::to_string(&database_reply) {
                        Ok(json) => json,
                        Err(e) => {
                            eprintln!("Error serializing: {}", e);
                            "".to_string()
                        }
                    };

                    // Append null terminator to tell the server when to stop reading
                    reply_json.push('\0');

                    if let Err(e) = socket.write_all(reply_json.as_bytes()).await {
                        eprintln!("Failed to send DatabaseReply: {}", e);
                    }
                }
                Err(e) => eprintln!("Error with the connection: {}", e),
            }
        });
    }
}
