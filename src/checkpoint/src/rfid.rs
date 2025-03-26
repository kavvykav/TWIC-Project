use std::process::Command;

pub fn write_rfid(id: u32) -> Result<bool, String> {
    let output = Command::new("python3")
        .arg("rfid.py")
        .arg("1")
        .arg(id.to_string())
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(true)
    } else {
        Err(format!(
            "Error: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

pub fn read_rfid() -> Result<u32, String> {
    let output = Command::new("python3")
        .arg("rfid.py")
        .arg("2")
        .output()
        .map_err(|e| format!("Failed to execute Python script: {}", e))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        println!("Raw stdout: {}", stdout); // Debug: Check what is actually received

        let data_str = stdout.trim(); // ✅ Remove extra whitespace

        // Ensure the data is numeric before parsing
        if data_str.chars().all(|c| c.is_digit(10)) {
            data_str
                .parse::<u32>()
                .map_err(|e| format!("Parse error: {}", e))
        } else {
            Err(format!(
                "Unexpected output from Python script: '{}'",
                data_str
            ))
        }
    } else {
        Err(format!(
            "Python script failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

pub fn get_token_id() -> Result<u64, String> {
    let output = Command::new("python3")
        .arg("rfid.py")
        .arg("3")
        .output()
        .map_err(|e| format!("Failed to execute Python script: {}", e))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        println!("Raw stdout: {}", stdout); // Debug: Check what is actually received

        let data_str = stdout.trim(); // ✅ Remove extra whitespace

        // Ensure the data is numeric before parsing
        if data_str.chars().all(|c| c.is_digit(10)) {
            data_str
                .parse::<u64>()
                .map_err(|e| format!("Parse error: {}", e))
        } else {
            Err(format!(
                "Unexpected output from Python script: '{}'",
                data_str
            ))
        }
    } else {
        Err(format!(
            "Python script failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
