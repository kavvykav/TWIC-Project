use serialport::{self, DataBits, Parity, StopBits, FlowControl};
use std::io::{Write};
use std::time::Duration;

/// Captures fingerprint data from the fingerprint sensor connected via serial port.
/// 
/// # Arguments
/// - `port_name`: The name of the serial port (e.g., `/dev/ttyUSB0` on Linux, `COM3` on Windows).
/// - `baud_rate`: The baud rate for communication (e.g., 9600).
/// 
/// # Returns
/// - `Ok(Vec<u8>)`: The fingerprint data as a vector of bytes.
/// - `Err(String)`: An error message if something goes wrong.
pub fn capture_fingerprint(port_name: &str, baud_rate: u32) -> Result<Vec<u8>, String> {
    // Open the serial port and configure it
    let mut port = serialport::new(port_name, baud_rate)
        .timeout(Duration::from_secs(10)) // Timeout after 10 seconds
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::One)
        .flow_control(FlowControl::None)
        .open()
        .map_err(|e| format!("Failed to open serial port: {}", e))?;

    println!("Connected to fingerprint sensor on {}", port_name);

    // Step 1: Send initialization command
    let init_command: [u8; 4] = [0x55, 0xAA, 0x01, 0x00]; // Modify based on your sensor protocol
    port.write_all(&init_command)
        .map_err(|e| format!("Failed to send init command: {}", e))?;
    println!("Initialization command sent.");

    // Step 2: Wait for acknowledgment
    let mut buffer = [0; 16]; // Adjust size based on expected response
    let bytes_read = port
        .read(&mut buffer)
        .map_err(|e| format!("Failed to read acknowledgment: {}", e))?;

    if bytes_read == 0 {
        return Err("No acknowledgment received.".to_string());
    }

    println!("Initialization acknowledged.");

    // Step 3: Send capture command
    let capture_command: [u8; 4] = [0x55, 0xAA, 0x01, 0x02]; // Modify based on your sensor protocol
    port.write_all(&capture_command)
        .map_err(|e| format!("Failed to send capture command: {}", e))?;
    println!("Capture command sent. Waiting for fingerprint...");

    // Step 4: Wait for fingerprint data (you might need to adjust the sleep time based on sensor specs)
    std::thread::sleep(Duration::from_secs(3));

    // Reading fingerprint data (size can vary)
    let mut fingerprint_data = vec![0; 512]; // Adjust size based on expected fingerprint data length
    let bytes_read = port
        .read(&mut fingerprint_data)
        .map_err(|e| format!("Failed to read fingerprint data: {}", e))?;

    if bytes_read == 0 {
        return Err("No fingerprint data received.".to_string());
    }

    println!("Fingerprint data received ({} bytes).", bytes_read);
    fingerprint_data.truncate(bytes_read); // Resize to actual data length

    Ok(fingerprint_data)
}
