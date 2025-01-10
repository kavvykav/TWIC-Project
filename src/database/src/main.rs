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
    let conn = Connection::open("registered_workers.db")?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS workers (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            location TEXT NOT NULL,
            role TEXT NOT NULL
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
                    "SELECT name || ',' || location || ',' || role FROM workers WHERE id = ?1",
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
                    let result = conn.execute(
                        "INSERT INTO workers (name, location, role) VALUES (?1, ?2, ?3)",
                        params![fields[0], fields[1], fields[2]],
                    );
                    match result {
                        Ok(_) => Response {
                            status: "success".to_string(),
                            data: None,
                        },
                        Err(_) => Response {
                            status: "error".to_string(),
                            data: Some("Failed to insert worker".to_string()),
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
        //TODO: Implement these functionalities
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
