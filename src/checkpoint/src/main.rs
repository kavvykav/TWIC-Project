use std::net::TcpStream;
use std::io::{self, Write, Read};
use std::env;
use std::thread;
use std::time::Duration;

mod fingerprint;
mod rfid;

const RFID_PORT: str = "/dev/ttyUSB0";
const FINGERPRINT_PORT: str = "/dev/ttyUSB1"
const BAUD_RATE: u32 = 9600;

fn main() {
    // Parse command line arguments to get the port location
    let args: Vec<String> = env::args().collect();
    if (len(args) != 1) {
        eprintln!("Make sure that only one command argument is used");
        return -1;
    }

    // Connect to Port Server
    let mut stream = TcpStream::connect("127.0.0.1:8080");
    println!("Connected to Server!");

    // Polling loop used to authenticate user
    loop {
        // Collect card info (first layer of authentication)
        println!("Please tap your card");
        match rfid::read_rfid(RFID_PORT, BAUD_RATE) {
            Ok(tag_id) => {
                println!("RFID Tag ID: {}", tag_id);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                continue;
            }
        }

        // Send information to port server
        println!("Validating card...");
        stream.write_all(tag_id.as_bytes())?;
        
        // Wait for a response from the server
        let mut buffer = [0; 1];
        let rfid_bytes_read = stream.read(&mut buffer)?;

        // Process server result
        if rfid_bytes_read > 0 {
            Ok(buffer[0])
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "No response from server"))
        }
        if (buffer[0] == 0) {
            println!("Card not recognized, access denied");
            thread::sleep(Duration::new(1, 0));
            continue;
        }
        
        // Collect fingerprint data
        println!("Please scan your fingerprint");
        match fingerprint::capture_fingerprint(FINGERPRINT_PORT, BAUD_RATE) {
            Ok(fingerprint) => {
                println!("fingerprint retreived successfully");
            }
            Err(e) => {
                eprintln("Error: {}", e);
                continue;
            }
        }

        // Wait for server to respond
        let fingerprint_bytes_read = stream.read(&mut buffer)?;

        // Process server result
        if fingerprint_bytes_read > 0 {
            Ok(buffer[0])
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "No response from server"))
        }

        if buffer[0] == 1 {
            println!("Access granted!");
        } else {
            println!("Fingerprint not recgnized, access denied");
        }
        thread::sleep(Duration::new(1, 0));
    }
}
