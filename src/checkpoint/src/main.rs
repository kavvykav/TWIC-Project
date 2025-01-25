/****************
    IMPORTS
****************/
use common::{CheckpointRequest, CheckpointReply, CheckpointState, Lcd, LCD_LINE_1, LCD_LINE_2};
use std::net::TcpStream;
use std::io::{Write, Read, BufRead, BufReader};
use std::env;
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
    let args: Vec<String> = env::args().collect();
    if args.len() <  4 {
        eprintln!("Command line arguments need to be as follows: [function] [location] [allowed roles]");
        return;
    }

    // Get location of the checkpoint
    let location = args.get(2).unwrap().to_string();

    // Get authorized roles for this checkpoint
    let authorized_roles = args[3..].to_vec().join(",");

    // Initialize LCD
    let mut lcd = match init_lcd() {
        Some(lcd) => lcd,
        None => return, // Exit if LCD initialization fails
    };

    // Connect to Port Server
    let mut stream = match TcpStream::connect("127.0.0.1:8080") {
        Ok(stream) => {
            println!("Connected to Server!");
            lcd.display_string("Connected!", LCD_LINE_1);
            thread::sleep(Duration::from_secs(2));
            lcd.clear();
            stream
        }
        Err(e) => {
            eprintln!("Failed to connect to server: {}", e);
            lcd.display_string("Connection Fail", LCD_LINE_1);
            thread::sleep(Duration::from_secs(2));
            lcd.clear();
            return;
        }
    };

    // Send an init request to register in the database
    let init_req = CheckpointRequest::init_request(location.clone(), authorized_roles);

    let init_reply: CheckpointReply = send_and_receive(&mut stream, &init_req);

    if init_reply.status == "error" {
        eprintln!("Error with registering the checkpoint");
        lcd.display_string("Init failed", LCD_LINE_1);
        thread::sleep(Duration::from_secs(2));
        lcd.clear();
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

                    lcd.display_string("Enter Name", LCD_LINE_1);
                    thread::sleep(Duration::from_secs(2));
                    lcd.clear();

                    let worker_name = "Jim Bob".to_string();
                    println!("Please enter your role");

                    lcd.display_string("Enter Role", LCD_LINE_1);
                    thread::sleep(Duration::from_secs(2));
                    lcd.clear();

                    let role_id = 2;
                    println!("Please scan your fingerprint");

                    lcd.display_string("Enter Your", LCD_LINE_1);
                    lcd.display_string("Fingerprint", LCD_LINE_2);
                    thread::sleep(Duration::from_secs(2));
                    lcd.clear();

                    let worker_fingerprint = "dummy fingerprint".to_string();
                    
                    // Construct enroll request
                    let enroll_req = CheckpointRequest::enroll_req(checkpoint_id, worker_name, worker_fingerprint, location, role_id);
        
                    // Send and receive the response
                    let enroll_reply = send_and_receive(&mut stream, &enroll_req);
                    lcd.display_string("Enrolling...", LCD_LINE_1);
                    thread::sleep(Duration::from_secs(2));
                    lcd.clear();

        
                    // Error check
                    if enroll_reply.status == "success".to_string() {
                        println!("User enrolled successfully!");
                        lcd.display_string("Enrolled", LCD_LINE_1);
                        lcd.display_string("Successfully!", LCD_LINE_2);
                        thread::sleep(Duration::from_secs(2));
                        lcd.clear();
                        return;
                    } else {
                        println!("Error with enrolling the user");
                        lcd.display_string("Error!", LCD_LINE_1);
                        thread::sleep(Duration::from_secs(2));
                        lcd.clear();

                        return;
                    }
        
        
                }
                "update" => {
                    println!("Please give your ID");

                    lcd.display_string("Please Scan", LCD_LINE_1);
                    lcd.display_string("your card", LCD_LINE_2);
                    thread::sleep(Duration::from_secs(2));
                    lcd.clear();

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
                        lcd.display_string("Updated", LCD_LINE_1);
                        lcd.display_string("Successfully!", LCD_LINE_2);
                        thread::sleep(Duration::from_secs(2));
                        lcd.clear();
                        return;
                    } else {
                        println!("Error with updating the user");
                        lcd.display_string("Error", LCD_LINE_1);
                        thread::sleep(Duration::from_secs(2));
                        lcd.clear();
                        return;
                    }
                }
        
                "delete" => {
                    println!("Please give your ID");

                    lcd.display_string("Please Scan", LCD_LINE_1);
                    lcd.display_string("your card", LCD_LINE_2);
                    thread::sleep(Duration::from_secs(2));
                    lcd.clear();

                    let worker_id = 1;
        
                    // Construct request structure
                    let delete_req = CheckpointRequest::delete_req(checkpoint_id, worker_id);
        
                    // Send request and receive response
                    let delete_reply = send_and_receive(&mut stream, &delete_req);
        
                    // Error check
                    if delete_reply.status == "success".to_string() {
                        println!("User deleted successfully!");
                        lcd.display_string("Deleted", LCD_LINE_1);
                        lcd.display_string("Successfully!", LCD_LINE_2);
                        thread::sleep(Duration::from_secs(2));
                        lcd.clear();
                        return;
                    } else {
                        println!("Error with deleting the user");
                        lcd.display_string("Error", LCD_LINE_1);
                        thread::sleep(Duration::from_secs(2));
                        lcd.clear();

                        return;
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

                        let rfid_auth_req = CheckpointRequest::rfid_auth_request(checkpoint_id, worker_id);
                        let auth_reply: CheckpointReply = send_and_receive(&mut stream, &rfid_auth_req);
                        if let Some(CheckpointState::AuthFailed) = auth_reply.auth_response {
                            println!("Authentication failed.");
                            lcd.clear();
                            lcd.display_string("Access Denied", LCD_LINE_1);
                            thread::sleep(Duration::from_secs(2));
                            lcd.clear();
                            continue;
                        } else {
                            println!("Please scan your fingerprint");
                            lcd.clear();
                            lcd.display_string("Please scan", LCD_LINE_1);
                            lcd.display_string("fingerprint", LCD_LINE_2);
                            thread::sleep(Duration::from_secs(2));
                            lcd.clear();
                            lcd.display_string("Validating", LCD_LINE_1);
                        }
                        
        
                        // Collect fingerprint data
                        let worker_fingerprint = "dummy fingerprint".to_string();
                        let fingerprint_auth_request= CheckpointRequest::fingerprint_auth_req(checkpoint_id, worker_id, worker_fingerprint);
                        let fingerprint_auth_reply: CheckpointReply = send_and_receive(&mut stream, &fingerprint_auth_request);
                        if let Some(CheckpointState::AuthFailed) = fingerprint_auth_reply.auth_response {
                            println!("Authentication failed.");
                            lcd.clear();
                            lcd.display_string("Access Denied", LCD_LINE_1);
                            thread::sleep(Duration::from_secs(2));
                            lcd.clear();
                            continue;
                        } else {
                            println!("Authentication successful");
                            lcd.clear();
                            lcd.display_string("Access Granted", LCD_LINE_1);
                            thread::sleep(Duration::from_secs(2));
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
