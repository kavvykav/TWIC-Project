
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::thread;
use std::sync::{Arc, Mutex};
use chrono::Local;  // Import chrono for timestamping

const SERVER_ADDR: &str = "127.0.0.1:7878";

pub fn start_client() -> io::Result<()> {
    // Open the log file in append mode (create it if it doesn't exist)
    let file = OpenOptions::new()
        .write(true)
        .append(true)
        .create(true) // This will create the file if it doesn't exist
        .open("client_log.txt")?;

    // Wrap the file inside Arc<Mutex<>> for thread-safe access
    let file = Arc::new(Mutex::new(file));

    // Attempt to connect to the server
    let stream = TcpStream::connect(SERVER_ADDR)
        .expect("Failed to connect to server");

    // Log it in the file
    let connection_message = format!("Connected to server at {} at {}\n", SERVER_ADDR, Local::now());

    file.lock().unwrap().write_all(connection_message.as_bytes())?;

    // Do the receiving in another thread so we can return the main thread and not block the main loop
    let stream = Arc::new(Mutex::new(stream));
    let file_clone = Arc::clone(&file);
    thread::spawn(move || {
        let mut buffer = [0; 512];

        // Try to read data from the stream
        match stream.lock().unwrap().read(&mut buffer) {
            Ok(0) => {
                // Log server disconnection message
                let disconnect_message = format!(
                    "Server disconnected at {}: {}\n",
                    SERVER_ADDR,
                    Local::now()
                );
                file_clone.lock().unwrap().write_all(disconnect_message.as_bytes()).unwrap();
            }
            Ok(n) => {
                let message = String::from_utf8_lossy(&buffer[..n]);
                // Log received message
                let log_message = format!(
                    "Received message: {} at {}: {}\n",
                    message,
                    SERVER_ADDR,
                    Local::now()
                );
                file_clone.lock().unwrap().write_all(log_message.as_bytes()).unwrap();
            }
            Err(e) => {
                // Log the error
                let error_message = format!(
                    "Error reading from server: {} at {}: {}\n",
                    e,
                    SERVER_ADDR,
                    Local::now()
                );
                file_clone.lock().unwrap().write_all(error_message.as_bytes()).unwrap();
            }
        }
    });

    Ok(())
}
