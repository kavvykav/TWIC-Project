/**********************************
            IMPORTS
**********************************/
use rppal::i2c::I2c;
use serde::{Deserialize, Serialize};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/*************************************
    ROLES FOR ROLE BASED AUTH
**************************************/
pub static ROLES: &[&str] = &["Admin", "Worker", "Manager", "Security"];

#[derive(Debug, PartialEq, Eq)]
pub struct Role;

impl Role {
    pub fn from_str(role: &str) -> Option<usize> {
        ROLES.iter().position(|&r| r.eq_ignore_ascii_case(role))
    }

    pub fn as_str(id: usize) -> Option<&'static str> {
        ROLES.get(id).copied()
    }

    pub fn all_roles() -> &'static [&'static str] {
        ROLES
    }
}

/***************************************
    CHECKPOINT <--> PORT SERVER
****************************************/

#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub enum CheckpointState {
    WaitForRfid,
    WaitForFingerprint,
    AuthSuccessful,
    AuthFailed,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CheckpointReply {
    pub status: String,
    pub checkpoint_id: Option<u32>,
    pub worker_id: Option<u32>,
    pub fingerprint: Option<String>,
    pub data: Option<String>,
    pub auth_response: Option<CheckpointState>,
}

#[derive(Serialize, Clone, Deserialize)]
pub struct CheckpointRequest {
    pub command: String,
    pub checkpoint_id: Option<u32>,
    pub worker_id: Option<u32>,
    pub worker_fingerprint: Option<String>,
    pub location: Option<String>,
    pub authorized_roles: Option<String>,
    pub role_id: Option<u32>,
    pub worker_name: Option<String>,
}

impl CheckpointRequest {
    pub fn init_request(location: String, authorized_roles: String) -> CheckpointRequest {
        return CheckpointRequest {
            command: "INIT_REQUEST".to_string(),
            checkpoint_id: None,
            worker_id: None,
            worker_fingerprint: None,
            location: Some(location),
            authorized_roles: Some(authorized_roles),
            role_id: None,
            worker_name: None,
        };
    }

    pub fn rfid_auth_request(checkpoint_id: u32, worker_id: u32) -> CheckpointRequest {
        return CheckpointRequest {
            command: "AUTHENTICATE".to_string(),
            checkpoint_id: Some(checkpoint_id),
            worker_id: Some(worker_id),
            worker_fingerprint: Some("dummy hash".to_string()),
            location: None,
            authorized_roles: None,
            role_id: None,
            worker_name: None,
        };
    }

    pub fn fingerprint_auth_req(
        checkpoint_id: u32,
        worker_id: u32,
        worker_fingerprint: String,
    ) -> CheckpointRequest {
        return CheckpointRequest {
            command: "AUTHENTICATE".to_string(),
            checkpoint_id: Some(checkpoint_id),
            worker_id: Some(worker_id),
            worker_fingerprint: Some(worker_fingerprint),
            location: None,
            authorized_roles: None,
            role_id: None,
            worker_name: None,
        };
    }

    pub fn enroll_req(
        checkpoint_id: u32,
        worker_name: String,
        worker_fingerprint: String,
        location: String,
        role_id: u32,
    ) -> CheckpointRequest {
        return CheckpointRequest {
            command: "ENROLL".to_string(),
            checkpoint_id: Some(checkpoint_id),
            worker_id: None,
            worker_fingerprint: Some(worker_fingerprint),
            location: Some(location),
            authorized_roles: None,
            role_id: Some(role_id),
            worker_name: Some(worker_name),
        };
    }

    pub fn update_req(
        checkpoint_id: u32,
        worker_id: u32,
        new_role_id: u32,
        new_location: String,
    ) -> CheckpointRequest {
        return CheckpointRequest {
            command: "UPDATE".to_string(),
            checkpoint_id: Some(checkpoint_id),
            worker_id: Some(worker_id),
            worker_fingerprint: None,
            location: Some(new_location),
            authorized_roles: None,
            role_id: Some(new_role_id),
            worker_name: None,
        };
    }

    pub fn delete_req(checkpoint_id: u32, worker_id: u32) -> CheckpointRequest {
        return CheckpointRequest {
            command: "DELETE".to_string(),
            checkpoint_id: Some(checkpoint_id),
            worker_id: Some(worker_id),
            worker_fingerprint: None,
            location: None,
            authorized_roles: None,
            role_id: None,
            worker_name: None,
        };
    }
}

impl CheckpointReply {
    pub fn error() -> CheckpointReply {
        return CheckpointReply {
            status: "error".to_string(),
            checkpoint_id: None,
            worker_id: None,
            fingerprint: None,
            data: None,
            auth_response: None,
        };
    }
    pub fn auth_reply(state: CheckpointState) -> Self {
        return CheckpointReply {
            status: "success".to_string(),
            checkpoint_id: None,
            worker_id: None,
            fingerprint: None,
            data: None,
            auth_response: Some(state),
        };
    }

    pub fn waiting() -> Self {
        return CheckpointReply {
            status: "waiting".to_string(),
            checkpoint_id: None,
            worker_id: None,
            fingerprint: None,
            data: None,
            auth_response: None,
        };
    }
}

/*********************************************
    PORT SERVER <--> CENTRAL DATABASE
*********************************************/
pub const SERVER_ADDR: &str = "127.0.0.1:8080";
pub const DATABASE_ADDR: &str = "127.0.0.1:3036";

// Client structure for a port server to manage checkpoints
#[derive(Clone)]
pub struct Client {
    pub id: usize,
    pub stream: Arc<Mutex<TcpStream>>,
    pub state: CheckpointState,
}

// Format for requests to the Database
#[derive(Deserialize, Serialize, Clone)]
pub struct DatabaseRequest {
    pub command: String,
    pub checkpoint_id: Option<u32>,
    pub worker_id: Option<u32>,
    pub worker_name: Option<String>,
    pub worker_fingerprint: Option<String>,
    pub location: Option<String>,
    pub authorized_roles: Option<String>,
    pub role_id: Option<u32>,
}

// Database response format

#[derive(Deserialize, Serialize, Clone)]
pub struct DatabaseReply {
    pub status: String,
    pub checkpoint_id: Option<u32>,
    pub worker_id: Option<u32>,
    pub worker_fingerprint: Option<String>,
    pub role_id: Option<u32>,
    pub authorized_roles: Option<String>,
    pub location: Option<String>,
    pub auth_response: Option<CheckpointState>,
    pub allowed_locations: Option<String>,
    pub worker_name: Option<String>,
}

impl DatabaseReply {
    pub fn success() -> Self {
        DatabaseReply {
            status: "success".to_string(),
            checkpoint_id: None,
            worker_id: None,
            worker_fingerprint: None,
            role_id: None,
            authorized_roles: None,
            location: None,
            auth_response: None,
            allowed_locations: None,
            worker_name: None,
        }
    }

    pub fn update_success(allowed_locations: String, role_id: u32) -> Self {
        DatabaseReply {
            status: "success".to_string(),
            checkpoint_id: None,
            worker_id: None,
            worker_fingerprint: None,
            role_id: Some(role_id),
            authorized_roles: None,
            location: None,
            auth_response: None,
            allowed_locations: Some(allowed_locations),
            worker_name: None,
        }
    }

    pub fn error() -> Self {
        DatabaseReply {
            status: "error".to_string(),
            checkpoint_id: None,
            worker_id: None,
            worker_fingerprint: None,
            role_id: None,
            authorized_roles: None,
            location: None,
            auth_response: None,
            allowed_locations: None,
            worker_name: None,
        }
    }
    pub fn auth_reply(
        checkpoint_id: u32,
        worker_id: u32,
        worker_fingerprint: String,
        role_id: u32,
        authorized_roles: String,
        location: String,
        allowed_locations: String,
        worker_name: String,
    ) -> Self {
        DatabaseReply {
            status: "success".to_string(),
            checkpoint_id: Some(checkpoint_id),
            worker_id: Some(worker_id),
            worker_fingerprint: Some(worker_fingerprint),
            role_id: Some(role_id),
            authorized_roles: Some(authorized_roles),
            location: Some(location),
            auth_response: None,
            allowed_locations: Some(allowed_locations),
            worker_name: Some(worker_name),
        }
    }
    pub fn init_reply(checkpoint_id: u32) -> Self {
        DatabaseReply {
            status: "success".to_string(),
            checkpoint_id: Some(checkpoint_id),
            worker_id: None,
            worker_fingerprint: None,
            role_id: None,
            authorized_roles: None,
            location: None,
            auth_response: None,
            allowed_locations: None,
            worker_name: None,
        }
    }
}

/**************************
*      LCD DISPLAY
*************************/
const LCD_ADDR: u16 = 0x27; // Default I2C address for most 1602 I2C LCDs
const LCD_CHR: u8 = 1;
const LCD_CMD: u8 = 0;
pub const LCD_LINE_1: u8 = 0x80; // Line 1 start
pub const LCD_LINE_2: u8 = 0xC0; // Line 2 start
const LCD_BACKLIGHT: u8 = 0x08; // On
const ENABLE: u8 = 0b00000100;

pub struct Lcd {
    i2c: I2c,
}

impl Lcd {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut i2c = I2c::new()?;
        i2c.set_slave_address(LCD_ADDR)?;
        let lcd = Lcd { i2c };
        lcd.init();
        Ok(lcd)
    }

    pub fn init(&self) {
        self.write_byte(0x33, LCD_CMD); // Initialize
        self.write_byte(0x32, LCD_CMD); // Set to 4-bit mode
        self.write_byte(0x06, LCD_CMD); // Cursor move direction
        self.write_byte(0x0C, LCD_CMD); // Turn cursor off
        self.write_byte(0x28, LCD_CMD); // 2-line display
        self.write_byte(0x01, LCD_CMD); // Clear display
        thread::sleep(Duration::from_millis(2));
    }

    pub fn write_byte(&self, bits: u8, mode: u8) {
        let high_nibble = mode | (bits & 0xF0) | LCD_BACKLIGHT;
        let low_nibble = mode | ((bits << 4) & 0xF0) | LCD_BACKLIGHT;

        self.i2c_write(high_nibble);
        self.enable_pulse(high_nibble);

        self.i2c_write(low_nibble);
        self.enable_pulse(low_nibble);
    }

    pub fn i2c_write(&self, data: u8) {
        if let Err(e) = self.i2c.block_write(0, &[data]) {
            eprintln!("I2C write error: {:?}", e);
        }
    }

    pub fn enable_pulse(&self, data: u8) {
        self.i2c_write(data | ENABLE);
        thread::sleep(Duration::from_micros(500));
        self.i2c_write(data & !ENABLE);
        thread::sleep(Duration::from_micros(500));
    }

    pub fn clear(&self) {
        self.write_byte(0x01, LCD_CMD);
        thread::sleep(Duration::from_millis(2));
    }

    pub fn display_string(&self, text: &str, line: u8) {
        self.write_byte(line, LCD_CMD);
        for c in text.chars() {
            self.write_byte(c as u8, LCD_CHR);
        }
    }
}
