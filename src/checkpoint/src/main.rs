/****************
    IMPORTS
****************/
use common::{CheckpointReply, CheckpointRequest, CheckpointState, Submission};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::exit;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[cfg(feature = "raspberry_pi")]
use common::{Lcd, LCD_LINE_1, LCD_LINE_2};

mod fingerprint;
mod rfid;

/*
 * Name: init_lcd
 * Function: Wrapper function to initialize the LCD.
 */
#[cfg(feature = "raspberry_pi")]
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

#[cfg(not(feature = "raspberry_pi"))]
fn init_lcd() -> Option<()> {
    println!("LCD not supported on this device.");
    None
}

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
    rfid_ver: Option<bool>,
) -> CheckpointReply {
    println!("Sending request: {:?}", request); // Debug log

    let rfid_ver = rfid_ver.unwrap_or(false); //Could handle with Some and if for each case also

    // Special case: Skip two-admin approval for initialization or auth requests
    if request.command == "INIT_REQUEST" || request.command == "AUTHENTICATE" {
        println!("Initialization request detected. Skipping two-admin approval.");

        let mut json = match serde_json::to_string(request) {
            Ok(json) => json,
            Err(e) => {
                eprintln!("Could not serialize structure: {}", e);
                return CheckpointReply::error();
            }
        };

        // Print the JSON before sending
        println!("Sending JSON request: {}", json);

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
    }

    // For non-init requests, use the two-admin approval logic
    let request_key = format!(
        "{}_{}_{}",
        request.command,
        request.worker_id.unwrap_or(0),
        request.checkpoint_id.unwrap_or(0)
    );
    let mut pending = pending_requests.try_lock();
    if !pending.is_ok() {
        eprintln!("Could not acquire lock, skipping request.");
        return CheckpointReply::error();
    }
    let mut pending = pending.unwrap();

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
 * Name: format_fingerprint_json
 * Function: formats the json to be sent to port server
 */
fn format_fingerprint_json(checkpoint_id: u32, fingerprint_id: u32) -> Value {
    json!({
        "fingerprints": {
            checkpoint_id.to_string(): fingerprint_id
        }
    })
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
    #[cfg(feature = "raspberry_pi")]
    let lcd = match init_lcd() {
        Some(lcd) => lcd,
        None => return, // Exit if LCD initialization fails
    };

    // Connect to Port Server
    let mut stream = match TcpStream::connect("127.0.0.1:8080") {
        Ok(stream) => {
            println!("Connected to Server!");
            #[cfg(feature = "raspberry_pi")]
            {
                lcd.display_string("Connected!", LCD_LINE_1);
                thread::sleep(Duration::from_secs(5));
                lcd.clear();
            }
            stream
        }
        Err(e) => {
            eprintln!("Failed to connect to server: {}", e);
            #[cfg(feature = "raspberry_pi")]
            {
                lcd.display_string("Connection to", LCD_LINE_1);
                lcd.display_string("server failed", LCD_LINE_2);
                thread::sleep(Duration::from_secs(5));
                lcd.clear();
            }
            return;
        }
    };

    // Example admin IDs
    let admin_id_1 = 1; // First admin
    let admin_id_2 = 2; // Second admin

    // Send an init request to register in the database
    let init_req = CheckpointRequest::init_request(location.clone(), authorized_roles);

    let rfid_ver = Some(false);

    let mut init_reply: CheckpointReply = send_and_receive(
        &mut stream,
        &init_req,
        pending_requests.clone(),
        admin_id_1,
        rfid_ver,
    );

    if init_reply == CheckpointReply::error() {
        lcd.clear();
        eprintln!("Failed to connect to server, exiting");
        exit(1);
    }

    println!(
        "DEBUG: checkpoint_id received = {:?}",
        init_reply.checkpoint_id
    );

    // Handle the "waiting" status
    while init_reply.status == "waiting" {
        println!("Waiting for another admin to approve the request...");

        // Sleep for a few seconds before retrying
        thread::sleep(Duration::from_secs(5));

        // Retry sending the request
        init_reply = send_and_receive(
            &mut stream,
            &init_req,
            pending_requests.clone(),
            admin_id_1,
            rfid_ver,
        );
    }

    if init_reply.status != "success" {
        eprintln!(
            "Error with registering the checkpoint: {}",
            init_reply.status
        );
        #[cfg(feature = "raspberry_pi")]
        {
            lcd.display_string("Init failed", LCD_LINE_1);
            thread::sleep(Duration::from_secs(5));
            lcd.clear();
        }
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
                    let worker_id: u64;
                    let result = match rfid::get_token_id() {
                        Ok(val) => {
                            worker_id = val;
                        }
                        Err(e) => {
                            eprintln!("Error: {e}");
                            exit(1);
                        }
                    };
                    let rfid_data: u32;
                    let result = match rfid::read_rfid() {
                        Ok(val) => {
                            rfid_data = val;
                        }
                        Err(e) => {
                            eprintln!("Error: {e}");
                            exit(1);
                        }
                    };

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

                                    let fingerprint_json = format_fingerprint_json(
                                        checkpoint_id,
                                        biometric.parse::<u32>().unwrap_or(0), // Convert biometric to fingerprint ID (Does this work ok?)
                                    );

                                    let enroll_req = CheckpointRequest::enroll_req(
                                        checkpoint_id,
                                        name,
                                        worker_id,
                                        rfid_data,
                                        serde_json::to_string(&fingerprint_json).unwrap(),
                                        location,
                                        role_id,
                                    );

                                    // First admin sends the request
                                    let enroll_reply_1 = send_and_receive(
                                        &mut stream,
                                        &enroll_req,
                                        Arc::clone(&pending_requests.clone()),
                                        admin_id_1,
                                        rfid_ver,
                                    );

                                    if enroll_reply_1 == CheckpointReply::error() {
                                        eprintln!("Failed to connect to server, exiting");
                                        lcd.clear();
                                        exit(1);
                                    }

                                    if enroll_reply_1.status == "waiting" {
                                        // Second admin approves the request
                                        let enroll_reply_2 = send_and_receive(
                                            &mut stream,
                                            &enroll_req,
                                            Arc::clone(&pending_requests.clone()),
                                            admin_id_2,
                                            rfid_ver,
                                        );

                                        if enroll_reply_2 == CheckpointReply::error() {
                                            eprintln!("Failed to connect to server, exiting");
                                            exit(1);
                                        }

                                        if enroll_reply_2.status == "success" {
                                            println!("User enrolled successfully");
                                            #[cfg(feature = "raspberry_pi")]
                                            {
                                                lcd.display_string("Enrolled", LCD_LINE_1);
                                                lcd.display_string("Successfully", LCD_LINE_2);
                                            }
                                        } else {
                                            eprintln!("Error enrolling user: {:?}", enroll_reply_2); // Debug log
                                            #[cfg(feature = "raspberry_pi")]
                                            {
                                                lcd.display_string("Error!", LCD_LINE_1);
                                            }
                                        }
                                    } else {
                                        eprintln!("Error enrolling user: {:?}", enroll_reply_1); // Debug log
                                        #[cfg(feature = "raspberry_pi")]
                                        {
                                            lcd.display_string("Error!", LCD_LINE_1);
                                        }
                                    }
                                }
                                Submission::Update {
                                    employee_id,
                                    role_id,
                                } => {
                                    let role_id = role_id.parse::<u32>().unwrap_or(0);

                                    let update_req = CheckpointRequest::update_req(
                                        checkpoint_id,
                                        worker_id,
                                        role_id,
                                        location.clone(),
                                    );

                                    // First admin sends the request
                                    let update_reply_1 = send_and_receive(
                                        &mut stream,
                                        &update_req,
                                        Arc::clone(&pending_requests.clone()),
                                        admin_id_1,
                                        rfid_ver,
                                    );
                                    if update_reply_1 == CheckpointReply::error() {
                                        lcd.clear();
                                        eprintln!("Failed to connect to server, exiting");
                                        exit(1);
                                    }

                                    if update_reply_1.status == "waiting" {
                                        // Second admin approves the request
                                        let update_reply_2 = send_and_receive(
                                            &mut stream,
                                            &update_req,
                                            Arc::clone(&pending_requests.clone()),
                                            admin_id_2,
                                            rfid_ver,
                                        );
                                        if update_reply_2 == CheckpointReply::error() {
                                            lcd.clear();
                                            eprintln!("Failed to connect to server, exiting");
                                            exit(1);
                                        }

                                        if update_reply_2.status == "success" {
                                            println!("User updated successfully");
                                            #[cfg(feature = "raspberry_pi")]
                                            {
                                                lcd.display_string("Updated", LCD_LINE_1);
                                            }
                                        } else {
                                            eprintln!("Error updating user: {:?}", update_reply_2); // Debug log
                                            #[cfg(feature = "raspberry_pi")]
                                            {
                                                lcd.display_string("Error!", LCD_LINE_1);
                                            }
                                        }
                                    } else {
                                        eprintln!("Error updating user: {:?}", update_reply_1); // Debug log
                                        #[cfg(feature = "raspberry_pi")]
                                        {
                                            lcd.display_string("Error!", LCD_LINE_1);
                                        }
                                    }
                                }
                                Submission::Delete { employee_id } => {

                                    let delete_req =
                                        CheckpointRequest::delete_req(checkpoint_id, worker_id);

                                    // First admin sends the request
                                    let delete_reply_1 = send_and_receive(
                                        &mut stream,
                                        &delete_req,
                                        Arc::clone(&pending_requests.clone()),
                                        admin_id_1,
                                        rfid_ver,
                                    );

                                    if delete_reply_1 == CheckpointReply::error() {
                                        eprintln!("Failed to connect to server, exiting");
                                        exit(1);
                                    }

                                    if delete_reply_1.status == "waiting" {
                                        // Second admin approves the request
                                        let delete_reply_2 = send_and_receive(
                                            &mut stream,
                                            &delete_req,
                                            Arc::clone(&pending_requests.clone()),
                                            admin_id_2,
                                            rfid_ver,
                                        );

                                        if delete_reply_2 == CheckpointReply::error() {
                                            lcd.clear();
                                            eprintln!("Failed to connect to server, exiting");
                                            exit(1);
                                        }

                                        if delete_reply_2.status == "success" {
                                            println!("User deleted successfully!");
                                            #[cfg(feature = "raspberry_pi")]
                                            {
                                                lcd.display_string("Deleted", LCD_LINE_1);
                                            }
                                        } else {
                                            eprintln!("Error Deleting user: {:?}", delete_reply_2); // Debug log
                                            #[cfg(feature = "raspberry_pi")]
                                            {
                                                lcd.display_string("Error!", LCD_LINE_1);
                                            }
                                        }
                                    } else {
                                        eprintln!("Error Deleting user: {:?}", delete_reply_1); // Debug log
                                        #[cfg(feature = "raspberry_pi")]
                                        {
                                            lcd.display_string("Error!", LCD_LINE_1);
                                        }
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

                        #[cfg(feature = "raspberry_pi")]
                        {
                            lcd.display_string("Please Scan", LCD_LINE_1);
                        }

                        let (worker_id, rfid_data) = match (rfid::get_token_id(), rfid::read_rfid())
                        {
                            (Ok(w_id), Ok(rfid)) => (w_id, rfid),
                            _ => {
                                println!("Error reading RFID");
                                #[cfg(feature = "raspberry_pi")]
                                {
                                    lcd.clear();
                                    lcd.display_string("Scan Error", LCD_LINE_1);
                                    thread::sleep(Duration::from_secs(2));
                                    lcd.clear();
                                }
                                continue;
                            }
                        };

                        // Send information to port server
                        println!("Validating card...");
                        #[cfg(feature = "raspberry_pi")]
                        {
                            lcd.clear();
                            lcd.display_string("Validating", LCD_LINE_1);
                        }

                        let rfid_auth_req = CheckpointRequest::rfid_auth_request(
                            checkpoint_id,
                            worker_id,
                            rfid_data,
                        );

                        let auth_reply = send_and_receive(
                            &mut stream,
                            &rfid_auth_req,
                            pending_requests.clone(),
                            admin_id_1,
                            rfid_ver,
                        );

                        if auth_reply == CheckpointReply::error() {
                            lcd.clear();
                            eprintln!("Failed to connect to server, exiting");
                            exit(1);
                        }

                        if auth_reply.auth_response == Some(CheckpointState::AuthFailed) {
                            println!("Authentication failed.");
                            #[cfg(feature = "raspberry_pi")]
                            {
                                lcd.clear();
                                lcd.display_string("Access Denied", LCD_LINE_1);
                                thread::sleep(Duration::from_secs(2));
                                lcd.clear();
                            }
                            continue;
                        }

                        println!("RFID Authentication Succeeded");
                        println!("Please scan your fingerprint");
                        #[cfg(feature = "raspberry_pi")]
                        {
                            lcd.clear();
                            lcd.display_string("Please scan", LCD_LINE_1);
                            lcd.display_string("fingerprint", LCD_LINE_2);
                        }

                        // Collect fingerprint data
                        let worker_fingerprint: String;
                        match fingerprint::scan_fingerprint() {
                            Ok(fingerprint_id) => worker_fingerprint = fingerprint_id.to_string(),
                            Err(e) => {
                                println!("Error scanning fingerprint: {}", e);
                                worker_fingerprint = 961.to_string();
                            }
                        };

                        #[cfg(feature = "raspberry_pi")]
                        {
                            lcd.clear();
                            lcd.display_string("Validating", LCD_LINE_1);
                        }

                        let fingerprint_auth_request = CheckpointRequest::fingerprint_auth_req(
                            checkpoint_id,
                            worker_id,
                            worker_fingerprint,
                        );

                        let fingerprint_auth_reply = send_and_receive(
                            &mut stream,
                            &fingerprint_auth_request,
                            pending_requests.clone(),
                            admin_id_1,
                            rfid_ver,
                        );

                        if fingerprint_auth_reply == CheckpointReply::error() {
                            lcd.clear();
                            eprintln!("Failed to connect to server, exiting");
                            exit(1);
                        }

                        if fingerprint_auth_reply.auth_response
                            == Some(CheckpointState::AuthFailed)
                        {
                            println!("Authentication failed.");
                            #[cfg(feature = "raspberry_pi")]
                            {
                                lcd.clear();
                                lcd.display_string("Access Denied", LCD_LINE_1);
                                thread::sleep(Duration::from_secs(2));
                                lcd.clear();
                            }
                        } else if fingerprint_auth_reply.auth_response == Some(CheckpointState::AuthSuccessful) {
                            println!("Authentication successful");
                            #[cfg(feature = "raspberry_pi")]
                            {
                                lcd.clear();
                                lcd.display_string("Access Granted", LCD_LINE_1);
                                thread::sleep(Duration::from_secs(2));
                                lcd.clear();
                            }
                        }

                        // Clear any residual state
                        thread::sleep(Duration::from_secs(1));
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
