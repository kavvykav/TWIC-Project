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
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        println!("Raw stdout: {}", stdout);
        let id_str = stdout.trim();
        if let Ok(id) = id_str.parse::<u32>() {
            Ok(id)
        } else {
            Err("Failed to parse RFID tag ID".to_string())
        }
    } else {
        Err(format!(
            "Error: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
