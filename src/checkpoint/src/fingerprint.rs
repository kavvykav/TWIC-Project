use std::process::Command;

/// Enrolls a fingerprint using `fpm.py`
pub fn enroll_fingerprint(id: u32) -> Result<bool, String> {
    let output = Command::new("python3")
        .arg("fpm.py")
        .arg("2")
        .arg(id.to_string())
        .output()
        .map_err(|e| format!("Failed to execute fingerprint enroll script: {}", e))?;

    if output.status.success() {
        Ok(true)
    } else {
        Err(format!(
            "Fingerprint enrollment failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Scans a fingerprint and returns the scanned fingerprint ID
pub fn scan_fingerprint() -> Result<u32, String> {
    let output = Command::new("python3")
        .arg("fpm.py")
        .arg("1")
        .output()
        .map_err(|e| format!("Failed to execute fingerprint scan script: {}", e))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let id_str = stdout.trim();

        if let Ok(id) = id_str.parse::<u32>() {
            Ok(id)
        } else {
            Err(format!(
                "Unexpected output from fingerprint scan: '{}'",
                id_str
            ))
        }
    } else {
        Err(format!(
            "Fingerprint scan failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
