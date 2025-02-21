use std::process::Command;
use std::str;

pub fn scan_rfid() -> Result<String, String> {
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
                Ok(result)
            }
        }
        Err(_) => Err("Failed to execute RFID script".to_string()),
    }
}
