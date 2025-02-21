
use std::process::Command;
use std::str;

pub fn scan_fingerprint() -> Result<String, String> {
    let output = Command::new("python3")
        .arg("/path/to/fingerprint.py")
        .output();

    match output {
        Ok(output) => {
            let result = str::from_utf8(&output.stdout).unwrap_or("").trim().to_string();
            if result == "ERROR" {
                Err("Failed to read fingerprint".to_string())
            } else {
                Ok(result)
            }
        }
        Err(_) => Err("Failed to execute fingerprint script".to_string()),
    }
}
