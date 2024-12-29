use std::net::TcpStream;
use std::io::{Write, Read};
use std::env;
use std::thread;
use std::time::Duration;

mod fingerprint;
mod rfid;

const RFID_PORT: &str = "/dev/ttyUSB0";
const FINGERPRINT_PORT: &str = "/dev/ttyUSB1";
const BAUD_RATE: u32 = 9600;

fn main() {
    // Parse command line arguments to get the port location
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Make sure that only one command argument is used");
        return;
    }

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
        let mut buffer = [0; 1];
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

        thread::sleep(Duration::new(1, 0));
    }
}
