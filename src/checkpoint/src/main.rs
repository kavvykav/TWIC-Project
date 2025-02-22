/****************
    IMPORTS
****************/
use common::{
    App, CheckpointReply, CheckpointRequest, CheckpointState, Lcd, Submission, LCD_LINE_1,
    LCD_LINE_2,
};
use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

mod fingerprint;
mod rfid;

/****************
    CONSTANTS
****************/
const RFID_PORT: &str = "/dev/ttyUSB0";
const FINGERPRINT_PORT: &str = "/dev/ttyUSB1";
const BAUD_RATE: u32 = 9600;

/*
 * Name: send_and_receive
 * Function: sends an init message to have the checkpoint register in the centralized database,
 *           where the checkpoint is assigned an ID.
 */
fn send_and_receive(
    stream: &mut TcpStream,
    request: &CheckpointRequest,
    pending_requests: Arc<Mutex<HashMap<String, u32>>>,
    admin_id: u32,
) -> CheckpointReply {
    println!("Sending request: {:?}", request); // Debug log
    let request_key = format!(
        "{}_{}_{}",
        request.command,
        request.worker_id.unwrap_or(0),
        request.checkpoint_id.unwrap_or(0)
    );

    let mut pending = pending_requests.lock().unwrap();

    if let Some(existing_admin) = pending.get(&request_key) {
        if *existing_admin != admin_id {
            // If a different admin sends the same request, proceed
            println!("Two admins confirmed request: {:?}", request.command);
            pending.remove(&request_key); // Remove from pending

            let mut json = match serde_json::to_string(request) {
                Ok(json) => json,
                Err(e) => {
                    eprintln!("Could not serialize structure: {}", e);
                    return CheckpointReply::error();
                }
            };
            json.push('\0');

            if let Err(e) = stream.write_all(json.as_bytes()) {
                eprintln!("Could not send to port server: {}", e);
                return CheckpointReply::error();
            }

            stream.flush().unwrap();

            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut buffer = Vec::new();
            let buffer_str: String = match reader.read_until(b'\0', &mut buffer) {
                Ok(_) => match String::from_utf8(buffer.clone()) {
                    Ok(mut string) => {
                        string.pop();
                        string
                    }
                    Err(e) => {
                        eprintln!("Failed to convert buffer to a string format: {}", e);
                        String::new()
                    }
                },
                Err(e) => {
                    eprintln!("Failed to read response from port server: {}", e);
                    String::new()
                }
            };

            let response: CheckpointReply = match serde_json::from_str(&buffer_str) {
                Ok(resp) => resp,
                Err(e) => {
                    eprintln!("Could not deserialize response: {}", e);
                    return CheckpointReply::error();
                }
            };

            return response;
        } else {
            // Same admin cannot approve their own request
            println!(
                "Admin {} tried to approve their own request again. Waiting for another admin.",
                admin_id
            );
            return CheckpointReply::waiting();
        }
    } else {
        // First admin makes the request
        pending.insert(request_key, admin_id);
        println!(
            "Admin {} initiated request: {:?}",
            admin_id, request.command
        );
        return CheckpointReply::waiting();
    }
}

/*
 * Name: init_lcd
 * Function: Wrapper function to initialize the LCD.
 */
fn init_lcd() -> Option<Lcd> {
    match Lcd::new() {
        Ok(lcd) => {
            println!("LCD initialized successfully.");
            Some(lcd)
        }
        Err(e) => {
            eprintln!("Failed to initialize LCD: {}", e);
            None
        }
    }
}

/*
 * Name: main
 * Function: serves as the main checkpoint logic
 */
fn main() {
    // Parse command line arguments to get the port location and roles that this
    // checkpoint allows
    let pending_requests: Arc<Mutex<HashMap<String, u32>>> = Arc::new(Mutex::new(HashMap::new()));
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!(
            "Command line arguments need to be as follows: [function] [location] [allowed roles]"
        );
        return;
    }

    // Get location of the checkpoint
    let location = args.get(2).unwrap().to_string();

    // Get authorized roles for this checkpoint
    let authorized_roles = args[3..].to_vec().join(",");

    // Initialize LCD
    let lcd = match init_lcd() {
        Some(lcd) => lcd,
        None => return, // Exit if LCD initialization fails
    };

    // Connect to Port Server
    let mut stream = match TcpStream::connect("127.0.0.1:8080") {
        Ok(stream) => {
            println!("Connected to Server!");
            lcd.display_string("Connected!", LCD_LINE_1);
            thread::sleep(Duration::from_secs(5));
            lcd.clear();
            stream
        }
        Err(e) => {
            eprintln!("Failed to connect to server: {}", e);
            lcd.display_string("Connection to", LCD_LINE_1);
            lcd.display_string("server failed", LCD_LINE_2);
            thread::sleep(Duration::from_secs(5));
            lcd.clear();
            return;
        }
    };

    let admin_id = 1; // Example admin ID
    let is_admin = true; // Example admin status

    // Send an init request to register in the database
    let init_req = CheckpointRequest::init_request(location.clone(), authorized_roles);

    let init_reply: CheckpointReply =
        send_and_receive(&mut stream, &init_req, pending_requests.clone(), admin_id);

    if init_reply.status == "error" {
        eprintln!("Error with registering the checkpoint");
        lcd.display_string("Init failed", LCD_LINE_1);
        thread::sleep(Duration::from_secs(5));
        lcd.clear();
        return;
    }

    println!(
        "Got an ID assigned by the central database: {}",
        init_reply.checkpoint_id.unwrap_or(0)
    );

    // Store ID
    if let Some(checkpoint_id) = init_reply.checkpoint_id {
        // Functionalities at the checkpoint side
        if let Some(function) = args.get(1) {
            match function.as_str() {
                "tui" => {
                    // Call the TUI
                    match common::App::new().run() {
                        Ok(Some(submission)) => {
                            println!("TUI Submission received: {:?}", submission);
                            match submission {
                                Submission::Enroll {
                                    name,
                                    biometric,
                                    role_id,
                                    location,
                                } => {
                                    let role_id = role_id.parse::<u32>().unwrap_or(0);
        
                                    let enroll_req = CheckpointRequest::enroll_req(
                                        checkpoint_id,
                                        name,
                                        biometric,
                                        location,
                                        role_id,
                                    );
        
                                    let enroll_reply = send_and_receive(
                                        &mut stream,
                                        &enroll_req,
                                        Arc::clone(&pending_requests.clone()),
                                        admin_id,
                                    );
        
                                    if enroll_reply.status == "success" {
                                        println!("User enrolled successfully");
                                        lcd.display_string("Enrolled", LCD_LINE_1);
                                        lcd.display_string("Successfully", LCD_LINE_2);
                                    } else {
                                        eprintln!("Error enrolling user: {:?}", enroll_reply); // Debug log
                                        lcd.display_string("Error!", LCD_LINE_1);
                                    }
                                }
                                Submission::Update {
                                    employee_id,
                                    role_id,
                                } => {
                                    let role_id = role_id.parse::<u32>().unwrap_or(0);
                                    let employee_id = employee_id.parse::<u32>().unwrap_or(0);
        
                                    let update_req = CheckpointRequest::update_req(employee_id, role_id);
        
                                    let update_reply = send_and_receive(
                                        &mut stream,
                                        &update_req,
                                        Arc::clone(&pending_requests.clone()),
                                        admin_id,
                                    );
        
                                    if update_reply.status == "success" {
                                        println!("User updated successfully");
                                        lcd.display_string("Updated", LCD_LINE_1);
                                    } else {
                                        eprintln!("Error updating user: {:?}", update_reply); // Debug log
                                        lcd.display_string("Error!", LCD_LINE_1);
                                    }
                                }
                                Submission::Delete { employee_id } => {
                                    let employee_id = employee_id.parse::<u32>().unwrap_or(0);
        
                                    let delete_req = CheckpointRequest::delete_req(employee_id);
        
                                    let delete_reply = send_and_receive(
                                        &mut stream,
                                        &delete_req,
                                        Arc::clone(&pending_requests.clone()),
                                        admin_id,
                                    );
        
                                    if delete_reply.status == "success" {
                                        println!("User deleted successfully!");
                                        lcd.display_string("Deleted", LCD_LINE_1);
                                    } else {
                                        eprintln!("Error Deleting user: {:?}", update_reply); // Debug log
                                        lcd.display_string("Error!", LCD_LINE_1);
                                    }
                                }
                            }
                        }
                        Ok(None) => {
                            println!("TUI was exited without submission.");
                        }
                        Err(e) => {
                            eprintln!("TUI encountered an error: {}", e);
                        }
                    }
                }
        
                "authenticate" => {
                    // Polling loop used to authenticate user
                    loop {
                        // Collect card info (first layer of authentication)
                        println!("Please tap your card");

                        lcd.display_string("Please Scan", LCD_LINE_1);
                        thread::sleep(Duration::from_secs(2));
                        lcd.clear();

                        let worker_id = 1;

                        // Send information to port server
                        println!("Validating card...");
                        lcd.display_string("Validating", LCD_LINE_1);

                        let location = if is_admin {
                            "AdminSystem".to_string()
                        } else {
                            location.clone()
                        };
                        let rfid_auth_req =
                            CheckpointRequest::rfid_auth_request(checkpoint_id, worker_id);
                        let auth_reply: CheckpointReply = send_and_receive(
                            &mut stream,
                            &rfid_auth_req,
                            pending_requests.clone(),
                            admin_id,
                        );

                        if let Some(CheckpointState::AuthFailed) = auth_reply.auth_response {
                            eprintln!("RFID Authentication failed: {:?}", auth_reply); // Debug log
                            println!("Authentication failed.");
                            lcd.clear();
                            lcd.display_string("Access Denied", LCD_LINE_1);
                            thread::sleep(Duration::from_secs(5));
                            lcd.clear();
                            continue;
                        } else {
                            println!("Please scan your fingerprint");
                            lcd.clear();
                            lcd.display_string("Please scan", LCD_LINE_1);
                            lcd.display_string("fingerprint", LCD_LINE_2);
                            thread::sleep(Duration::from_secs(5));
                            lcd.clear();
                            lcd.display_string("Validating", LCD_LINE_1);
                        }

                        // Collect fingerprint data
                        let worker_fingerprint = "dummy fingerprint".to_string();
                        let fingerprint_auth_request = CheckpointRequest::fingerprint_auth_req(
                            checkpoint_id,
                            worker_id,
                            worker_fingerprint,
                        );
                        let fingerprint_auth_reply: CheckpointReply = send_and_receive(
                            &mut stream,
                            &fingerprint_auth_request,
                            pending_requests.clone(),
                            admin_id,
                        );
                        if let Some(CheckpointState::AuthFailed) =
                            fingerprint_auth_reply.auth_response
                        {
                            eprintln!("Fingerprint Authentication failed: {:?}", fingerprint_auth_reply); // Debug log
                            println!("Authentication failed.");
                            lcd.clear();
                            lcd.display_string("Access Denied", LCD_LINE_1);
                            thread::sleep(Duration::from_secs(5));
                            lcd.clear();
                            continue;
                        } else {
                            println!("Authentication successful");
                            lcd.clear();
                            lcd.display_string("Access Granted", LCD_LINE_1);
                            thread::sleep(Duration::from_secs(5));
                            lcd.clear();
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
