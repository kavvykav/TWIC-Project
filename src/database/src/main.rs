use rusqlite::{params, Connection, Result};
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

// IP address and port the device hosting the database is listening on
const IP_ADDRESS: &str = "127.0.0.1:3036";

// Request Format
#[derive(Deserialize)]
struct Request {
    command: String,
    data: Option<String>
}

// Response Format
#[derive(Serialize)]
struct Response {
    status: String,
    data: Option<String>,
}

/// Initializes the worker database if it does not already exist.
fn initialize_database() -> Result<Connection> {
    let conn = Connection::open("system.db")?;

    // Create roles table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS roles (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL
        )",
        [],
    )?;

    // Create ports table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ports (
            id INTEGER PRIMARY KEY,
            location TEXT NOT NULL
        )",
        [],
    )?;

    // Create employees table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS employees (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            fingerprint_hash TEXT NOT NULL,
            role_id INTEGER NOT NULL,
            FOREIGN KEY (role_id) REFERENCES roles (id)
        )",
        [],
    )?;

    // Create checkpoints table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS checkpoints (
            id INTEGER PRIMARY KEY,
            restricted_area TEXT NOT NULL,
            rsa_public_key INTEGER NOT NULL,
            port_id INTEGER NOT NULL,
            FOREIGN KEY (port_id) REFERENCES ports (id)
        )",
        [],
    )?;

    // Join table: checkpoint_roles
    conn.execute(
        "CREATE TABLE IF NOT EXISTS checkpoint_roles (
            checkpoint_id INTEGER NOT NULL,
            role_id INTEGER NOT NULL,
            PRIMARY KEY (checkpoint_id, role_id),
            FOREIGN KEY (checkpoint_id) REFERENCES checkpoints (id),
            FOREIGN KEY (role_id) REFERENCES roles (id)
        )",
        [],
    )?;

    // Join table: port_checkpoints
    conn.execute(
        "CREATE TABLE IF NOT EXISTS port_checkpoints (
            port_id INTEGER NOT NULL,
            checkpoint_id INTEGER NOT NULL,
            PRIMARY KEY (port_id, checkpoint_id),
            FOREIGN KEY (port_id) REFERENCES ports (id),
            FOREIGN KEY (checkpoint_id) REFERENCES checkpoints (id)
        )",
        [],
    )?;

    // Join table: port_employees
    conn.execute(
        "CREATE TABLE IF NOT EXISTS port_employees (
            port_id INTEGER NOT NULL,
            employee_id INTEGER NOT NULL,
            PRIMARY KEY (port_id, employee_id),
            FOREIGN KEY (port_id) REFERENCES ports (id),
            FOREIGN KEY (employee_id) REFERENCES employees (id)
        )",
        [],
    )?;

    Ok(conn)
}


/// Handles a request from the port server, whether it be an enrollment or
/// an authentication.
async fn handle_port_server_request(conn: Arc<Mutex<Connection>>, req: Request) -> Response {
    let conn = conn.lock().await;

    match req.command.as_str() {
        "AUTHENTICATE" => {
            if let Some(data) = req.data {
                let result: Result<String, _> = conn.query_row(
                    "SELECT name || ',' || location || ',' || role FROM employees WHERE id = ?1",
                    params![data],
                    |row| row.get(0),
                );
                match result {
                    Ok(worker_data) => Response {
                        status: "success".to_string(),
                        data: Some(worker_data),
                    },
                    Err(_) => Response {
                        status: "not found".to_string(),
                        data: None,
                    },
                }
            } else {
                Response {
                    status: "error".to_string(),
                    data: Some("ID not provided".to_string()),
                }
            }
        }
        "ENROLL" => {
    if let Some(data) = req.data {
        let fields: Vec<&str> = data.split(',').collect();
        if fields.len() == 3 {
            let name = fields[0];
            let location = fields[1]; // use this for linking employees to ports
            let role_name = fields[2];

            // Find the role_id for the role name
            let role_id_result: Result<i32, _> = conn.query_row(
                "SELECT id FROM roles WHERE name = ?1",
                params![role_name],
                |row| row.get(0),
            );

            match role_id_result {
                Ok(role_id) => {
                    // Insert the employee into the employees table
                    let insert_result = conn.execute(
                        "INSERT INTO employees (name, fingerprint_hash, role_id) VALUES (?1, ?2, ?3)",
                        params![name, "dummy_hash", role_id], // Replace this later
                    );

                    match insert_result {
                        Ok(_) => Response {
                            status: "success".to_string(),
                            data: None,
                        },
                        Err(e) => {
                            println!("Error enrolling employee: {}", e);
                            Response {
                                status: "error".to_string(),
                                data: Some("Failed to insert employee".to_string()),
                            }
                        }
                    }
                }
                Err(_) => Response {
                    status: "error".to_string(),
                    data: Some("Role not found".to_string()),
                },
            }
        } else {
            Response {
                status: "error".to_string(),
                data: Some("Invalid data format".to_string()),
            }
        }
    } else {
        Response {
            status: "error".to_string(),
            data: Some("No data provided".to_string()),
        }
    }
}
  //do this later
        "DELETE" => {
            Response {
                status: "error".to_string(),
                data: Some("Not implemented yet".to_string()),
            }
        }
        "UPDATE" => {
            Response {
                status: "error".to_string(),
                data: Some("Not implemented yet".to_string()),
            }
        }
        _ => Response {
            status: "error".to_string(),
            data: Some("Unknown command".to_string()),
        },
    }
}

#[tokio::main] // Ensures an async runtime is set up for the program
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database = initialize_database()?; // Handle database initialization
    let database = Arc::new(Mutex::new(database)); // Wrap the connection in Arc<Mutex>

    let listener = TcpListener::bind(IP_ADDRESS).await?;
    println!("Database server is listening on {}", IP_ADDRESS);

    loop {
        let (mut socket, addr) = listener.accept().await?;
        println!("Accepted connection from {}", addr);

        let database = Arc::clone(&database); // Clone Arc for each task

        tokio::spawn(async move {
            let mut buffer = vec![0; 1024];

            match socket.read(&mut buffer).await {
                Ok(0) => println!("Client at {} has closed the connection", addr),
                Ok(n) => {
                    let request_json = String::from_utf8_lossy(&buffer[..n]);
                    let request: Result<Request, _> = serde_json::from_str(&request_json);

                    let response = match request {
                        Ok(req) => handle_port_server_request(database, req).await,
                        Err(_) => Response {
                            status: "error".to_string(),
                            data: Some("Invalid request format".to_string()),
                        },
                    };

                    let response_json = serde_json::to_string(&response).unwrap();

                    if let Err(e) = socket.write_all(response_json.as_bytes()).await {
                        eprintln!("Failed to send response: {}", e);
                    }
                }
                Err(e) => eprintln!("Error with the connection: {}", e),
            }
        });
    }
}
