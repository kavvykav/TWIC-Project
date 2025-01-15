/****************
    IMPORTS
****************/
use std::net::TcpStream;
use std::io::{Write, Read, BufRead, BufReader};
use std::env;
use std::thread;
use std::time::Duration;
use serde::{Deserialize, Serialize};

mod fingerprint;
mod rfid;

/****************
    CONSTANTS
****************/
const RFID_PORT: &str = "/dev/ttyUSB0";
const FINGERPRINT_PORT: &str = "/dev/ttyUSB1";
const BAUD_RATE: u32 = 9600;

/****************
    STRUCTURES
****************/
#[derive(Deserialize, Clone)]
struct CheckpointReply {
    status: String,
    checkpoint_id: Option<u32>,
    worker_id: Option<u32>,
    fingerprint: Option<String>,
    data: Option<String>,
}
#[derive(Serialize, Clone)]
struct CheckpointRequest {
    command: String,
    checkpoint_id: Option<u32>,
    worker_id: Option<u32>,
    worker_fingerprint: Option<String>,
    location: Option<String>,
    authorized_roles: Option<String>,
}

/*
 * Name: register_in_database
 * Function: sends an init message to have the checkpoint register in the centralized database,
 *           where the checkpoint is assigned an ID.
 */
fn register_in_database(stream: &mut TcpStream, init_req: &CheckpointRequest) -> CheckpointReply {

    // Serialize structure into a JSON
    let mut init_json = match serde_json::to_string(init_req) {
        Ok(json) => json,
        Err(e) => {
            eprintln!("Could not serialize structure: {}", e);
            return CheckpointReply {
                status: "error".to_string(),
                checkpoint_id: None,
                worker_id: None,
                fingerprint: None,
                data: None,
            }
        }
    };
    init_json.push('\0');

    // Send to Port Server
    if let Err(e) = stream.write_all(init_json.as_bytes()) {
        eprintln!("Could not send to port server: {}", e);
        return CheckpointReply {
            status: "error".to_string(),
            checkpoint_id: None,
            worker_id: None,
            fingerprint: None,
            data: None,
        }
    }

    // Flush the stream
    stream.flush().unwrap();

    // Read response
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut buffer = Vec::new();

    let buffer_str: String = match reader.read_until(b'\0', &mut buffer) {
        Ok(_) => {
            match String::from_utf8(buffer.clone()) {
                Ok(mut string) => {
                    string.pop(); // Remove the null terminator if present
                    string
                }
                Err(e) => {
                    eprintln!("Failed to convert buffer to a string format: {}", e);
                    String::new()
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to read response from port server: {}", e);
            String::new()
        }
    };

    // Deserialize the response
    let response: CheckpointReply = match serde_json::from_str(&buffer_str) {
        Ok(resp) => resp,
        Err(e) => {
            eprintln!("Could not deserialize response: {}", e);
            return CheckpointReply {
                status: "error".to_string(),
                checkpoint_id: None,
                worker_id: None,
                fingerprint: None,
                data: None,
            }
        }
    };

    return response;
}

/* 
 * Name: main
 * Funnction: serves as the main checkpoint logic
 */
fn main() {
    // Parse command line arguments to get the port location and roles that this
    // checkpoint allows
    let args: Vec<String> = env::args().collect();
    if args.len() <  3 {
        eprintln!("Command line arguments need to be as follows: [location] [allowed roles]");
        return;
    }

    // Get location of the checkpoint
    let location = args.get(1).unwrap().to_string();

    // Get authorized roles for this checkpoint
    let authorized_roles = args[2..].to_vec().join(",");


    // Connect to Port Server
    let mut stream = match TcpStream::connect("127.0.0.1:8080") {
        Ok(stream) => {
            println!("Connected to Server!");
            stream
        }
        Err(e) => {
            eprintln!("Failed to connect to server: {}", e);
            return;
        }
    };

    // Send an init request to register in the database
    let init_req = CheckpointRequest {
        command: "INIT_REQUEST".to_string(),
        checkpoint_id: None,
        worker_id: None,
        worker_fingerprint: None,
        location: Some(location),
        authorized_roles: Some(authorized_roles),
    };

    let init_reply: CheckpointReply = register_in_database(&mut stream, &init_req);

    if init_reply.status == "error" {
        eprintln!("Error with registering the checkpoint");
        return;
    }

    println!("Got an ID assigned by the central database: {}", init_reply.checkpoint_id.unwrap_or(0) );

    // Polling loop used to authenticate user
    loop {
        // Collect card info (first layer of authentication)
        //println!("Please tap your card");
        let tag_id = match rfid::read_rfid(RFID_PORT, BAUD_RATE) {
            Ok(tag_id) => {
                println!("RFID Tag ID: {}", tag_id);
                tag_id
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        };

        // Send information to port server
        println!("Validating card...");
        if let Err(e) = stream.write_all(tag_id.as_bytes()) {
            eprintln!("Failed to send RFID data: {}", e);
            break;
        }

        // Wait for a response from the server
        let mut buffer = [0; 128];
        let rfid_bytes_read = match stream.read(&mut buffer) {
            Ok(bytes_read) => bytes_read,
            Err(e) => {
                eprintln!("Failed to read from server: {}", e);
                break;
            }
        };

        // Process server result
        if rfid_bytes_read > 0 {
            if buffer[0] == 0 {
                println!("Card not recognized, access denied");
                thread::sleep(Duration::new(1, 0));
                break;
            }
        } else {
            eprintln!("No response from server");
            break;
        }

        // Collect fingerprint data
        println!("Please scan your fingerprint");
        match fingerprint::capture_fingerprint(FINGERPRINT_PORT, BAUD_RATE) {
            Ok(_fingerprint) => {
                println!("Fingerprint retrieved successfully");
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }

        // Wait for server to respond
        let fingerprint_bytes_read = match stream.read(&mut buffer) {
            Ok(bytes_read) => bytes_read,
            Err(e) => {
                eprintln!("Failed to read from server: {}", e);
                break;
            }
        };

        // Process server result
        if fingerprint_bytes_read > 0 {
            if buffer[0] == 1 {
                println!("Access granted!");
            } else {
                println!("Fingerprint not recognized, access denied");
            }
        } else {
            eprintln!("No response from server");
        }

        // 5 second timeout between loop iterations
        thread::sleep(Duration::new(5, 0));
    }
}
