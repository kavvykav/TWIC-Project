mod roles;

use roles::Role;
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

const IP_ADDRESS: &str = "127.0.0.1:3036";

#[derive(Deserialize)]
struct Request {
    command: String,
    data: Option<String>,
}

#[derive(Serialize)]
struct Response {
    status: String,
    data: Option<String>,
}

fn str_to_int(input: &str) -> Result<i32, String> {
    input
        .trim()
        .parse::<i32>()
        .map_err(|_| format!("Invalid integer: {}", input))
}

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
            fingerprint_hash TEXT NOT NULL,
            role_id INTEGER NOT NULL,
            FOREIGN KEY (role_id) REFERENCES roles (id)
        )",
        [],
    )?;

    Ok(conn)
}

async fn handle_port_server_request(conn: Arc<Mutex<Connection>>, req: Request) -> Response {
    let conn = conn.lock().await;

    match req.command.as_str() {
        "AUTHENTICATE" => {
            if let Some(data) = req.data {
                let result: Result<String, _> = conn.query_row(
                    "SELECT employees.name || ',' || employees.fingerprint_hash || ',' || roles.name FROM employees \
                     JOIN roles ON employees.role_id = roles.id WHERE employees.id = ?1",
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
                    let location = fields[1];
                    let role_name = fields[2];

                    if let Some(role_id) = Role::from_str(role_name) {
                        let exists: bool = conn.query_row(
                            "SELECT EXISTS(SELECT 1 FROM employees WHERE name = ?1 AND role_id = ?2)",
                            params![name, role_id as i32],
                            |row| row.get(0),
                        ).unwrap_or(false);

                        if exists {
                            return Response {
                                status: "error".to_string(),
                                data: Some("Employee already exists".to_string()),
                            };
                        }

                        let result = conn.execute(
                            "INSERT INTO employees (name, fingerprint_hash, role_id) VALUES (?1, ?2, ?3)",
                            params![name, "dummy_hash", role_id as i32],
                        );

                        match result {
                            Ok(_) => Response {
                                status: "success".to_string(),
                                data: None,
                            },
                            Err(_) => Response {
                                status: "error".to_string(),
                                data: Some("Failed to enroll employee".to_string()),
                            },
                        }
                    } else {
                        Response {
                            status: "error".to_string(),
                            data: Some("Invalid role".to_string()),
                        }
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
        "UPDATE" => {
            if let Some(data) = req.data {
                let fields: Vec<&str> = data.split(',').collect();
                if fields.len() == 2 {
                    let id = str_to_int(fields[0]);
                    let new_role_name = fields[1];

                    if let Ok(id) = id {
                        if let Some(new_role_id) = Role::from_str(new_role_name) {
                            let result = conn.execute(
                                "UPDATE employees SET role_id = ?1 WHERE id = ?2",
                                params![new_role_id as i32, id],
                            );

                            match result {
                                Ok(affected) => {
                                    if affected > 0 {
                                        Response {
                                            status: "success".to_string(),
                                            data: Some(format!("Updated employee ID {} to role {}", id, new_role_name)),
                                        }
                                    } else {
                                        Response {
                                            status: "error".to_string(),
                                            data: Some("No employee found with the given ID".to_string()),
                                        }
                                    }
                                }
                                Err(_) => Response {
                                    status: "error".to_string(),
                                    data: Some("Failed to update employee".to_string()),
                                },
                            }
                        } else {
                            Response {
                                status: "error".to_string(),
                                data: Some("Invalid role".to_string()),
                            }
                        }
                    } else {
                        Response {
                            status: "error".to_string(),
                            data: Some("Invalid ID format".to_string()),
                        }
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
        "DELETE" => {
            if let Some(data) = req.data {
                let id = str_to_int(&data);

                if let Ok(id) = id {
                    let result = conn.execute("DELETE FROM employees WHERE id = ?1", params![id]);
                    match result {
                        Ok(affected) => {
                            if affected > 0 {
                                Response {
                                    status: "success".to_string(),
                                    data: Some(format!("Deleted employee with ID {}", id)),
                                }
                            } else {
                                Response {
                                    status: "error".to_string(),
                                    data: Some("No employee found with the given ID".to_string()),
                                }
                            }
                        }
                        Err(_) => Response {
                            status: "error".to_string(),
                            data: Some("Failed to delete employee".to_string()),
                        },
                    }
                } else {
                    Response {
                        status: "error".to_string(),
                        data: Some("Invalid ID format".to_string()),
                    }
                }
            } else {
                Response {
                    status: "error".to_string(),
                    data: Some("No data provided".to_string()),
                }
            }
        }
        _ => Response {
            status: "error".to_string(),
            data: Some("Unknown command".to_string()),
        },
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database = initialize_database()?;
    let database = Arc::new(Mutex::new(database));

    let listener = TcpListener::bind(IP_ADDRESS).await?;
    println!("Database server is listening on {}", IP_ADDRESS);

    loop {
        let (mut socket, addr) = listener.accept().await?;
        println!("Accepted connection from {}", addr);

        let database = Arc::clone(&database);

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
