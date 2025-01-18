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
#[derive(Deserialize, Serialize, Clone)]
enum CheckpointState {
    WaitForRfid,
    WaitForFingerprint,
    AuthSuccessful,
    AuthFailed,
}

#[derive(Deserialize, Serialize, Clone)]
enum EnrollUpdateDeleteStatus {
    Success,
    Failed,
}

#[derive(Deserialize, Clone)]
struct CheckpointReply {
    status: String,
    checkpoint_id: Option<u32>,
    worker_id: Option<u32>,
    fingerprint: Option<String>,
    data: Option<String>,
    auth_response: Option<CheckpointState>,
    update_delete_enroll_result: Option<EnrollUpdateDeleteStatus>,
}

#[derive(Serialize, Clone)]
struct CheckpointRequest {
    command: String,
    checkpoint_id: Option<u32>,
    worker_id: Option<u32>,
    worker_fingerprint: Option<String>,
    location: Option<String>,
    authorized_roles: Option<String>,
    role_id: Option<u32>,
    worker_name: Option<String>,
}

/****************
    WRAPPERS
****************/
impl CheckpointRequest {
    pub fn init_request(location: String, authorized_roles: String) -> CheckpointRequest {
        return CheckpointRequest {
            command: "INIT_REQUEST".to_string(),
            checkpoint_id: None,
            worker_id: None,
            worker_fingerprint: None,
            location: Some(location),
            authorized_roles: Some(authorized_roles),
            role_id: None,
            worker_name: None,
        };
    }

    pub fn rfid_auth_request(checkpoint_id: u32,
                      worker_id: u32,) -> CheckpointRequest {
        return CheckpointRequest {
            command: "AUTHENTICATE".to_string(),
            checkpoint_id: Some(checkpoint_id),
            worker_id: Some(worker_id),
            worker_fingerprint: Some("dummy hash".to_string()),
            location: None,
            authorized_roles: None,
            role_id: None,
            worker_name: None,
        };
    }

    pub fn fingerprint_auth_req(checkpoint_id: u32,
                                worker_id: u32,
                                worker_fingerprint: String) -> CheckpointRequest {
        
        return CheckpointRequest {
            command: "AUTHENTICATE".to_string(),
            checkpoint_id: Some(checkpoint_id),
            worker_id: Some(worker_id),
            worker_fingerprint: Some(worker_fingerprint),
            location: None,
            authorized_roles: None,
            role_id: None,
            worker_name: None,
        };
    }

    pub fn enroll_req(checkpoint_id: u32,
                      worker_name: String,
                      worker_fingerprint: String,
                      location: String,
                      role_id: u32) -> CheckpointRequest {
        return CheckpointRequest {
            command: "ENROLL".to_string(),
            checkpoint_id: Some(checkpoint_id),
            worker_id: None,
            worker_fingerprint: Some(worker_fingerprint),
            location: Some(location),
            authorized_roles: None,
            role_id: Some(role_id),
            worker_name: Some(worker_name),
        };
    }

    pub fn update_req(checkpoint_id: u32,
                      worker_id: u32,
                      new_role_id: u32,
                      new_location: String) -> CheckpointRequest {
        return CheckpointRequest {
            command: "UPDATE".to_string(),
            checkpoint_id: Some(checkpoint_id),
            worker_id: Some(worker_id),
            worker_fingerprint: None,
            location: Some(new_location),
            authorized_roles: None,
            role_id: Some(new_role_id),
            worker_name: None,
        };
    }
    
    pub fn delete_req(checkpoint_id: u32,
                      worker_id: u32) -> CheckpointRequest {
        return CheckpointRequest {
            command: "DELETE".to_string(),
            checkpoint_id: Some(checkpoint_id),
            worker_id: Some(worker_id),
            worker_fingerprint: None,
            location: None,
            authorized_roles: None,
            role_id: None,
            worker_name: None,
        };
    }
}
    

impl CheckpointReply {
    pub fn error() -> CheckpointReply {
        return CheckpointReply {
            status: "error".to_string(),
            checkpoint_id: None,
            worker_id: None,
            fingerprint: None,
            data: None,
            auth_response: None,
            update_delete_enroll_result: None,
        };
    }
}

/*
 * Name: send_and_receive
 * Function: sends an init message to have the checkpoint register in the centralized database,
 *           where the checkpoint is assigned an ID.
 */
fn send_and_receive(stream: &mut TcpStream, init_req: &CheckpointRequest) -> CheckpointReply {

    // Serialize structure into a JSON
    let mut json = match serde_json::to_string(init_req) {
        Ok(json) => json,
        Err(e) => {
            eprintln!("Could not serialize structure: {}", e);
            return CheckpointReply::error();
        }
    };
    json.push('\0');

    // Send to Port Server
    if let Err(e) = stream.write_all(json.as_bytes()) {
        eprintln!("Could not send to port server: {}", e);
        return CheckpointReply::error();
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
                    string.pop();
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
            return CheckpointReply::error();
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
    if args.len() <  4 {
        eprintln!("Command line arguments need to be as follows: [function] [location] [allowed roles]");
        return;
    }

    // Get location of the checkpoint
    let location = args.get(2).unwrap().to_string();

    // Get authorized roles for this checkpoint
    let authorized_roles = args[3..].to_vec().join(",");


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
    let init_req = CheckpointRequest::init_request(location.clone(), authorized_roles);

    let init_reply: CheckpointReply = send_and_receive(&mut stream, &init_req);

    if init_reply.status == "error" {
        eprintln!("Error with registering the checkpoint");
        return;
    }

    println!("Got an ID assigned by the central database: {}", init_reply.checkpoint_id.unwrap_or(0) );

    // Store ID
    if let Some(checkpoint_id) = init_reply.checkpoint_id {
        // Functionalities at the checkpoint side
        if let Some(function) = args.get(1) {
            match function.as_str() {
                "enroll" => {
                    println!("Please give your name");
                    let worker_name = "Jim Bob".to_string();
                    println!("Please enter your role");
                    let role_id = 2;
                    println!("Please scan your fingerprint");
                    let worker_fingerprint = "dummy fingerprint".to_string();
                    
                    // Construct enroll request
                    let enroll_req = CheckpointRequest::enroll_req(checkpoint_id, worker_name, worker_fingerprint, location, role_id);
        
                    // Send and receive the response
                    let enroll_reply = send_and_receive(&mut stream, &enroll_req);
        
                    // Error check
                    if enroll_reply.status == "success".to_string() {
                        println!("User enrolled successfully!");
                        thread::sleep(Duration::new(5, 0));
                        return;
                    } else {
                        println!("Error with enrolling the user");
                        thread::sleep(Duration::new(5, 0));
                        return;
                    }
        
        
                }
                "update" => {
                    println!("Please give your ID");
                    let worker_id = 1;
                    let new_role_id = 3;
                    let new_location = "Halifax".to_string();
        
                    // Construct request structure
                    let update_req = CheckpointRequest::update_req(checkpoint_id, worker_id, new_role_id, new_location);
        
                    // Send request and receive response
                    let update_reply = send_and_receive(&mut stream, &update_req);
        
                    // Error check
                    if update_reply.status == "success".to_string() {
                        println!("User updated successfully!");
                        thread::sleep(Duration::new(5, 0));
                        return;
                    } else {
                        println!("Error with updating the user");
                        thread::sleep(Duration::new(5, 0));
                        return;
                    }
                }
        
                "delete" => {
                    println!("Please give your ID");
                    let worker_id = 1;
        
                    // Construct request structure
                    let delete_req = CheckpointRequest::delete_req(checkpoint_id, worker_id);
        
                    // Send request and receive response
                    let delete_reply = send_and_receive(&mut stream, &delete_req);
        
                    // Error check
                    if delete_reply.status == "success".to_string() {
                        println!("User deleted successfully!");
                        thread::sleep(Duration::new(5, 0));
                        return;
                    } else {
                        println!("Error with deleting the user");
                        thread::sleep(Duration::new(5, 0));
                        return;
                    }
                }
                "authenticate" => {
                    // Polling loop used to authenticate user
                    loop {
                        // Collect card info (first layer of authentication)
                        println!("Please tap your card");
                        let worker_id = 1;

                        // Send information to port server
                        println!("Validating card...");
                        let rfid_auth_req = CheckpointRequest::rfid_auth_request(checkpoint_id, worker_id);
                        let auth_reply: CheckpointReply = send_and_receive(&mut stream, &rfid_auth_req);
                        if let Some(CheckpointState::AuthFailed) = auth_reply.auth_response {
                            println!("Authentication failed.");
                            thread::sleep(Duration::new(5, 0));
                            continue;
                        } else {
                            println!("Please scan your fingerprint");
                            thread::sleep(Duration::new(5, 0));
                        }
                        
        
                        // Collect fingerprint data
                        let worker_fingerprint = "Dummy fingerprint".to_string();
                        let fingerprint_auth_request= CheckpointRequest::fingerprint_auth_req(checkpoint_id, worker_id, worker_fingerprint);
                        let fingerprint_auth_reply: CheckpointReply = send_and_receive(&mut stream, &rfid_auth_req);
                        if let Some(CheckpointState::AuthFailed) = fingerprint_auth_reply.auth_response {
                            println!("Authentication failed.");
                            thread::sleep(Duration::new(5, 0));
                            continue;
                        } else {
                            println!("Authentication successful");
                        }                
        
                        // 5 second timeout between loop iterations
                        thread::sleep(Duration::new(5, 0));
                    }
                }
                _ => {
                    println!("Unknown function!");
                    return;
                }
            }
        }
    }

    
}