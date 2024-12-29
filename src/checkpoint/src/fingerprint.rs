use serialport::prelude::*;
use std::io::{self, Write};
use std::time::Duration;

pub fn capture_fingerprint(port_name: &str, baud_rate: u32) -> Result<Vec<u8>, String> {
    // Initialize the serial port
    let settings = SerialPortSettings {
        baud_rate,
        data_bits: DataBits::Eight,
        parity: Parity::None,
        stop_bits: StopBits::One,
        flow_control: FlowControl::None,
        timeout: Duration::from_secs(10),
    };

    let mut port = serialport::open_with_settings(port_name, &settings)
        .map_err(|e| format!("Failed to open serial port: {}", e))?;

    println!("Connected to fingerprint sensor on {}", port_name);

    // Step 1: Send initialization command
    let init_command: [u8; 4] = [0x55, 0xAA, 0x01, 0x00]; // Modify based on your sensor protocol
    port.write_all(&init_command)
        .map_err(|e| format!("Failed to send init command: {}", e))?;
    println!("Initialization command sent.");

    // Step 2: Wait for acknowledgment
    let mut buffer = [0; 16]; // Adjust size based on expected response
    let _bytes_read = port
        .read(buffer.as_mut_slice())
        .map_err(|e| format!("Failed to read acknowledgment: {}", e))?;
    println!("Initialization acknowledged.");

    // Step 3: Send capture command
    let capture_command: [u8; 4] = [0x55, 0xAA, 0x01, 0x02]; // Modify based on your sensor protocol
    port.write_all(&capture_command)
        .map_err(|e| format!("Failed to send capture command: {}", e))?;
    println!("Capture command sent. Waiting for fingerprint...");

    // Step 4: Read fingerprint data
    std::thread::sleep(Duration::from_secs(3)); // Wait for the sensor to process
    let mut fingerprint_data = vec![0; 512]; // Adjust size based on expected fingerprint data length
    let bytes_read = port
        .read(fingerprint_data.as_mut_slice())
        .map_err(|e| format!("Failed to read fingerprint data: {}", e))?;

    println!("Fingerprint data received ({} bytes).", bytes_read);
    fingerprint_data.truncate(bytes_read); // Resize to actual data length
    Ok(fingerprint_data)
}
