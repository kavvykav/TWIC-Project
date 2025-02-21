mod fingerprint_test;
mod rfid_test;

fn main() {
    match fingerprint::scan_fingerprint() {
        Ok(fingerprint_id) => println!("Fingerprint ID: {}", fingerprint_id),
        Err(e) => eprintln!("{}", e),
    }

    match rfid::scan_rfid() {
        Ok(rfid_data) => println!("RFID Data: {}", rfid_data),
        Err(e) => eprintln!("{}", e),
    }
}
