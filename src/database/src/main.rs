/****************
    IMPORTS
****************/

mod roles;

use roles::Role;
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

/****************
    CONSTANTS
****************/

const IP_ADDRESS: &str = "127.0.0.1:3036";

/****************
    STRUCTURES
****************/
#[derive(Deserialize)]
struct Request {
    command: String,
    checkpoint_id: Option<u32>,
    worker_id: Option<u32>,
    worker_name: Option<String>,
    worker_fingerprint: Option<String>,
    location: Option<String>,
    authorized_roles: Option<String>,
    role_id: Option<u32>,
}

#[derive(Serialize)]
struct Response {
    status: String,
    checkpoint_id: Option<u32>,
    worker_id: Option<u32>,
    worker_fingerprint: Option<String>,
    location: Option<String>,
    authorized_roles: Option<String>,
    role_id: Option<u32>,
}

/*
 * Name: str_to_int
 * Function: converts a number in string representation to a signed 32 bit integer.
 */
fn str_to_int(input: &str) -> Result<i32, String> {
    input
        .trim()
        .parse::<i32>()
        .map_err(|_| format!("Invalid integer: {}", input))
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
            fingerprint_hash TEXT NOT NULL,
            role_id INTEGER NOT NULL,
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
 * Name: handle_port_server_request
 * Function: Searches for the command in the Request structure from the port server, 
 *           and services the request accordingly.
 */
async fn handle_port_server_request(conn: Arc<Mutex<Connection>>, req: Request) -> Response {
    let conn = conn.lock().await;

    match req.command.as_str() {
        "INIT_REQUEST" => {
            let result = conn.execute(
                "INSERT INTO checkpoints (location, allowed_roles) VALUES (?1, ?2)",
                params![req.location, req.authorized_roles],
            );
            match result {
                Ok(_) => {
                    println!(
                        "Added checkpoint to the database! ID is {}",
                        conn.last_insert_rowid()
                    );
                    return Response {
                        status: "success".to_string(),
                        checkpoint_id: Some(conn.last_insert_rowid() as u32),
                        worker_id: None,
                        worker_fingerprint: None,
                        location: None,
                        authorized_roles: None,
                        role_id: None,
                    };
                }
                Err(_) => {
                    return Response {
                        status: "error".to_string(),
                        checkpoint_id: None,
                        worker_id: None,
                        worker_fingerprint: None,
                        location: None,
                        authorized_roles: None,
                        role_id: None,
                    };
                }
            }
        }
        "AUTHENTICATE" => {
            let _result: Result<(Option<String>, Option<String>, Option<String>, Option<u32>), _> =
                conn.query_row(
                    "SELECT employees.name, employees.fingerprint_hash, roles.name, roles.id \
    FROM employees \
    JOIN roles ON employees.role_id = roles.id \
    WHERE employees.id = ?1",
                    params![req.worker_id],
                    |row| {
                        Ok((
                            row.get(0)?, // employees.name
                            row.get(1)?, // employees.fingerprint_hash
                            row.get(2)?, // roles.name
                            row.get(3)?, // roles.id
                        ))
                    },
                );
            match _result {
                Ok((name, fingerprint_hash, role_name, role_id)) => {
                    return Response {
                        status: "success".to_string(),
                        checkpoint_id: req.checkpoint_id,
                        worker_id: req.worker_id,
                        worker_fingerprint: fingerprint_hash,
                        location: req.location,
                        authorized_roles: req.authorized_roles,
                        role_id: role_id,
                    }
                }
                Err(_) => {
                    return Response {
                        status: "error".to_string(),
                        checkpoint_id: None,
                        worker_id: None,
                        worker_fingerprint: None,
                        location: None,
                        authorized_roles: None,
                        role_id: None,
                    }
                }
            }
        }
        "ENROLL" => {
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM employees WHERE name = ?1 AND role_id = ?2)",
                    params![req.worker_name, req.worker_id],
                    |row| row.get(0),
                )
                .unwrap_or(false);

            if exists {
                return Response {
                    status: "error".to_string(),
                    checkpoint_id: None,
                    worker_id: None,
                    worker_fingerprint: None,
                    location: None,
                    authorized_roles: None,
                    role_id: None,
                };
            }

            let result = conn.execute(
                "INSERT INTO employees (name, fingerprint_hash, role_id) VALUES (?1, ?2, ?3)",
                params![req.worker_name, req.worker_fingerprint, req.worker_id],
            );

            match result {
                Ok(result) => {
                    return Response {
                        status: "success".to_string(),
                        checkpoint_id: None,
                        worker_id: None,
                        worker_fingerprint: None,
                        location: None,
                        authorized_roles: None,
                        role_id: None,
                    };
                }

                Err(_) => {
                    return Response {
                        status: "error".to_string(),
                        checkpoint_id: None,
                        worker_id: None,
                        worker_fingerprint: None,
                        location: None,
                        authorized_roles: None,
                        role_id: None,
                    };
                }
            }
        }
        "UPDATE" => {
            let result = conn.execute(
                "UPDATE employees SET role_id = ?1 WHERE id = ?2",
                params![req.role_id, req.worker_id],
            );
            match result {
                Ok(affected) => {
                    if affected > 0 {
                        return Response {
                            status: "success".to_string(),
                            checkpoint_id: None,
                            worker_id: None,
                            worker_fingerprint: None,
                            location: None,
                            authorized_roles: None,
                            role_id: None,
                        };
                    } else {
                        return Response {
                            status: "error".to_string(),
                            checkpoint_id: None,
                            worker_id: None,
                            worker_fingerprint: None,
                            location: None,
                            authorized_roles: None,
                            role_id: None,
                        };
                    }
                }
                Err(_) => {
                    return Response {
                        status: "error".to_string(),
                        checkpoint_id: None,
                        worker_id: None,
                        worker_fingerprint: None,
                        location: None,
                        authorized_roles: None,
                        role_id: None,
                    };
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
                        return Response {
                            status: "success".to_string(),
                            checkpoint_id: None,
                            worker_id: None,
                            worker_fingerprint: None,
                            location: None,
                            authorized_roles: None,
                            role_id: None,
                        };
                    } else {
                        return Response {
                            status: "error".to_string(),
                            checkpoint_id: None,
                            worker_id: None,
                            worker_fingerprint: None,
                            location: None,
                            authorized_roles: None,
                            role_id: None,
                        };
                    }
                }
                Err(_) => {
                    return Response {
                        status: "error".to_string(),
                        checkpoint_id: None,
                        worker_id: None,
                        worker_fingerprint: None,
                        location: None,
                        authorized_roles: None,
                        role_id: None,
                    };
                }
            }
        }
        _ => {
            println!("Unknown command");
            return Response {
                status: "error".to_string(),
                checkpoint_id: None,
                worker_id: None,
                worker_fingerprint: None,
                location: None,
                authorized_roles: None,
                role_id: None,
            };
        }
    }
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
                            checkpoint_id: None,
                            worker_id: None,
                            worker_fingerprint: None,
                            location: None,
                            authorized_roles: None,
                            role_id: None,
                        },
                    };

                    let mut response_json = match serde_json::to_string(&response) {
                        Ok(json) => json,
                        Err(e) => {
                            eprintln!("Error serializing response: {}", e);
                            "".to_string()
                        }
                    };

                    response_json.push('\0');

                    if let Err(e) = socket.write_all(response_json.as_bytes()).await {
                        eprintln!("Failed to send response: {}", e);
                    }
                }
                Err(e) => eprintln!("Error with the connection: {}", e),
            }
        });
    }
}
