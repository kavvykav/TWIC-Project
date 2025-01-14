use std::net::TcpStream;
use std::io::{Write, Read};
use std::env;
use std::thread;
use std::time::Duration;
use serde::{Deserialize, Serialize};

mod fingerprint;
mod rfid;

const RFID_PORT: &str = "/dev/ttyUSB0";
const FINGERPRINT_PORT: &str = "/dev/ttyUSB1";
const BAUD_RATE: u32 = 9600;

#[derive(Serialize, Clone)]
struct InitRequest {
    command: String,
    location: String,
    authorized_roles: String,
}

#[derive(Deserialize, Clone)]
struct InitReply {
    id: i32,
}

#[derive(Serialize, Clone)]
struct CheckpointRequest {
    command: String,
    checkpoint_id: Option<u32>,
    worker_id: Option<u32>,
    worker_fingerprint: Option<String>,
}

/// Regsiter a checkpoint in the centralized database upon startup.
fn register_in_database(stream: &mut TcpStream, init_req: &InitRequest) -> InitReply {

    // Serialize InitReq structure into a JSON
    let mut init_json = match serde_json::to_string(init_req) {
        Ok(json) => json,
        Err(e) => return InitReply{id: -1}
    };
    init_json.push('\0');

    // Send to Port Server
    if let Err(e) = stream.write_all(init_json.as_bytes()) {
        return InitReply{id: -1}
    }

    // Flush the stream
    stream.flush().unwrap();

    // Read response
    let mut buffer = vec![0;64];
    let bytes_read = match stream.read(&mut buffer) {
        Ok(bytes) => bytes,
        Err(e) => return InitReply{id: -1}
    };

    // Deserialize the response
    let response: InitReply = match serde_json::from_slice(&buffer[..bytes_read]) {
        Ok(resp) => resp,
        Err(e) => return InitReply{id: -1}
    };

    return response;
}

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
    let init_req = InitRequest {
        command: "INIT_REQUEST".to_string(),
        location: location,
        authorized_roles: authorized_roles,
    };

    let init_reply: InitReply = register_in_database(&mut stream, &init_req);

    if init_reply.id == -1 {
        eprintln!("Error with registering the checkpoint");
        return;
    }

    // Polling loop used to authenticate user
    loop {
        // Collect card info (first layer of authentication)
        println!("Please tap your card");
        let tag_id = match rfid::read_rfid(RFID_PORT, BAUD_RATE) {
            Ok(tag_id) => {
                println!("RFID Tag ID: {}", tag_id);
                tag_id
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                continue;
            }
        };

        // Send information to port server
        println!("Validating card...");
        if let Err(e) = stream.write_all(tag_id.as_bytes()) {
            eprintln!("Failed to send RFID data: {}", e);
            continue;
        }

        // Wait for a response from the server
        let mut buffer = [0; 128];
        let rfid_bytes_read = match stream.read(&mut buffer) {
            Ok(bytes_read) => bytes_read,
            Err(e) => {
                eprintln!("Failed to read from server: {}", e);
                continue;
            }
        };

        // Process server result
        if rfid_bytes_read > 0 {
            if buffer[0] == 0 {
                println!("Card not recognized, access denied");
                thread::sleep(Duration::new(1, 0));
                continue;
            }
        } else {
            eprintln!("No response from server");
            continue;
        }

        // Collect fingerprint data
        println!("Please scan your fingerprint");
        match fingerprint::capture_fingerprint(FINGERPRINT_PORT, BAUD_RATE) {
            Ok(_fingerprint) => {
                println!("Fingerprint retrieved successfully");
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                continue;
            }
        }

        // Wait for server to respond
        let fingerprint_bytes_read = match stream.read(&mut buffer) {
            Ok(bytes_read) => bytes_read,
            Err(e) => {
                eprintln!("Failed to read from server: {}", e);
                continue;
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
