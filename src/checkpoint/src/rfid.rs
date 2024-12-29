use serialport::{self, DataBits, Parity, StopBits, FlowControl};
use std::io::{Write};
use std::time::Duration;

/// Reads RFID data from the RFID module connected via serial port.
/// 
/// # Arguments
/// - `port_name`: The name of the serial port (e.g., `/dev/ttyUSB0` on Linux, `COM3` on Windows).
/// - `baud_rate`: The baud rate for communication (e.g., 9600).
/// 
/// # Returns
/// - `Ok(String)`: The RFID tag ID as a string.
/// - `Err(String)`: An error message if something goes wrong.
pub fn read_rfid(port_name: &str, baud_rate: u32) -> Result<String, String> {
    // Open the serial port and configure it
    let mut port = serialport::new(port_name, baud_rate)
        .timeout(Duration::from_secs(10)) // Timeout after 10 seconds
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::One)
        .flow_control(FlowControl::None)
        .open()
        .map_err(|e| format!("Failed to open serial port: {}", e))?;

    println!("Connected to RFID reader on {}", port_name);

    // Send a command to the RFID reader (optional, depends on your module)
    let read_command: [u8; 2] = [0x02, 0x20]; // Replace with actual command for your module
    port.write_all(&read_command)
        .map_err(|e| format!("Failed to send read command: {}", e))?;
    println!("Read command sent. Waiting for RFID tag...");

    // Wait and read RFID data
    let mut buffer = vec![0; 128]; // Adjust size based on expected tag length
    let bytes_read = port
        .read(&mut buffer)
        .map_err(|e| format!("Failed to read RFID data: {}", e))?;

    // Parse the RFID data
    if bytes_read > 0 {
        buffer.truncate(bytes_read); // Remove unused part of the buffer
        let tag_id = String::from_utf8(buffer)
            .map_err(|_| "Failed to parse RFID data as UTF-8.".to_string())?;
        Ok(tag_id)
    } else {
        Err("No RFID data received.".to_string())
    }
}
