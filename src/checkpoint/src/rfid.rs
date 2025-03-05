use std::process::Command;
use std::str;

pub fn write_rfid(id: u32) -> Result<bool, String> {
    println!("Spawning Python script...");
    let output = Command::new("python3")
        .arg("rfid.py")
<<<<<<< HEAD
        .arg("1") // Write mode
        .arg(id.to_string()) // ID to write
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to execute RFID script: {}", e))?;
=======
        .arg("1")
        .arg(id.to_string())
        .output();
>>>>>>> parent of e81a177 (more bs)

    // Capture stderr
    let stderr = str::from_utf8(&output.stderr)
        .map_err(|e| format!("Failed to decode script stderr: {}", e))?;

    if !stderr.is_empty() {
        println!("Python script stderr: {}", stderr);
    }

    // Capture stdout
    let result = str::from_utf8(&output.stdout)
        .map_err(|e| format!("Failed to decode script output: {}", e))?
        .trim()
        .to_string();

    println!("Raw output from Python script: {:?}", result);

    // Check for errors in the script output
    if result == "ERROR" || result.is_empty() {
        return Err("Failed to write RFID: Script error".to_string());
    }

    // Parse the result as a boolean (assuming the script returns "true" or "false")
    match result.as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("Unexpected output from RFID script: {}", result)),
    }
}

pub fn read_rfid() -> Result<String, String> {
    println!("Spawning Python script...");
    let output = Command::new("python3")
        .arg("rfid.py")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to execute RFID script: {}", e))?;

    // Capture stderr
    let stderr = str::from_utf8(&output.stderr)
        .map_err(|e| format!("Failed to decode script stderr: {}", e))?;

    if !stderr.is_empty() {
        println!("Python script stderr: {}", stderr);
    }

    // Capture stdout
    let result = str::from_utf8(&output.stdout)
        .map_err(|e| format!("Failed to decode script output: {}", e))?
        .trim()
        .to_string();

    println!("Raw output from Python script: {:?}", result);

    // Check for errors in the script output
    if result == "ERROR" || result.is_empty() {
        return Err("Failed to read RFID: No tag detected or script error".to_string());
    }

    Ok(result)
}
