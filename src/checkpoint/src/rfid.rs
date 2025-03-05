use std::process::{Command, Stdio};
use std::str;

pub fn scan_rfid() -> Result<u32, String> {
    let output = Command::new("python3").arg("/path/to/rfid.py").output();

    match output {
        Ok(output) => {
            let result = str::from_utf8(&output.stdout)
                .unwrap_or("")
                .trim()
                .to_string();

            if result == "ERROR" {
                Err("Failed to read RFID".to_string())
            } else {
                // Attempt to parse the result as an integer
                match result.parse::<u32>() {
                    Ok(int_value) => Ok(int_value),
                    Err(_) => Err("Failed to convert RFID to integer".to_string()),
                }
            }
        }
        Err(_) => Err("Failed to execute RFID script".to_string()),
    }
}

pub fn write_rfid(id: u32) -> Result<bool, String> {
    let output = Command::new("python3")
        .arg("rfid.py")
        .arg("1")
        .arg(id.to_string())
        .stderr(Stdio::piped())
        .output();

    match output {
        Ok(output) => {
            let result = str::from_utf8(&output.stdout)
                .unwrap_or("")
                .trim()
                .to_string();

            if result == "ERROR" {
                Err("Failed to write RFID".to_string())
            } else {
                // Attempt to parse the result as an integer
                match result.parse::<bool>() {
                    Ok(val) => Ok(val),
                    Err(_) => Err("Failed to convert RFID to integer".to_string()),
                }
            }
        }
        Err(_) => Err("Failed to execute RFID script".to_string()),
    }
}
