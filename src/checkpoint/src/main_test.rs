/****************
    IMPORTS
****************/
use common::{CheckpointReply, CheckpointRequest, CheckpointState, Lcd, LCD_LINE_1, LCD_LINE_2};
use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

mod fingerprint;
mod rfid;
const TIMEOUT: Duration = Duration::from_secs(30);

/****************
    CONSTANTS
****************/
const RFID_PORT: &str = "/dev/ttyUSB0";
const FINGERPRINT_PORT: &str = "/dev/ttyUSB1";
const BAUD_RATE: u32 = 9600;

fn get_rfid() -> Option<String> {
    let start_time = Instant::now();
    while start_time.elapsed() < TIMEOUT {
        let output = Command::new("python3")
            .arg("rfid.py")
            .output()
            .expect("Failed to execute RFID script");

        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = output_str.lines().collect();

            if let Some(last_line) = lines.last() {
                return last_line.parse().ok(); // Directly parse ID
            }
        }
    }
    None
}

fn get_fingerprint() -> Option<u32> {
    let start_time = Instant::now();
    while start_time.elapsed() < TIMEOUT {
        let output = Command::new("python3")
            .arg("fpm.py 1")
            .arg("1")
            .output()
            .expect("Failed to execute fingerprint script");

        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = output_str.lines().collect();

            if let Some(last_line) = lines.last() {
                return last_line.parse().ok(); // Directly parse ID
            }
        }
    }
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
) -> CheckpointReply {
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
    let pending_requests: Arc<Mutex<HashMap<String, u32>>> = Arc::new(Mutex::new(HashMap::new()));
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!(
            "Command line arguments need to be as follows: [function] [location] [allowed roles]"
        );
        return;
    }

    let location = args.get(2).unwrap().to_string();
    let authorized_roles = args[3..].to_vec().join(",");

    let lcd = match init_lcd() {
        Some(lcd) => lcd,
        None => return,
    };

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

    let admin_id = 1;
    let is_admin = true;

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

    if let Some(checkpoint_id) = init_reply.checkpoint_id {
        if let Some(function) = args.get(1) {
            match function.as_str() {
                "authenticate" => loop {
                    lcd.display_string("Scan RFID", LCD_LINE_1);
                    if let Some(worker_id) = get_rfid() {
                        lcd.display_string("Validating...", LCD_LINE_1);

                        let auth_req = CheckpointRequest::rfid_auth_request(
                            checkpoint_id,
                            worker_id.parse().unwrap_or(0),
                        );
                        let auth_reply: CheckpointReply = send_and_receive(
                            &mut stream,
                            &auth_req,
                            pending_requests.clone(),
                            admin_id,
                        );

                        if let Some(CheckpointState::AuthFailed) = auth_reply.auth_response {
                            lcd.display_string("Access Denied", LCD_LINE_1);
                            thread::sleep(Duration::from_secs(5));
                            continue;
                        }

                        lcd.display_string("Scan Fingerprint", LCD_LINE_1);
                        if let Some(fp_id) = get_fingerprint() {
                            let fp_auth_req = CheckpointRequest::fingerprint_auth_req(
                                checkpoint_id,
                                worker_id.parse().unwrap_or(0),
                                fp_id.to_string(),
                            );

                            let fp_auth_reply: CheckpointReply = send_and_receive(
                                &mut stream,
                                &fp_auth_req,
                                pending_requests.clone(),
                                admin_id,
                            );

                            if let Some(CheckpointState::AuthFailed) = fp_auth_reply.auth_response {
                                lcd.display_string("Access Denied", LCD_LINE_1);
                                thread::sleep(Duration::from_secs(5));
                                continue;
                            }
                            lcd.display_string("Access Granted", LCD_LINE_1);
                        } else {
                            lcd.display_string("Fingerprint Timeout", LCD_LINE_1);
                        }
                    } else {
                        lcd.display_string("RFID Timeout", LCD_LINE_1);
                    }
                    thread::sleep(Duration::from_secs(5));
                },
                _ => {
                    println!("Unknown function!");
                    return;
                }
            }
        }
    }
}
