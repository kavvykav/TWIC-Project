/**********************************
            IMPORTS
**********************************/
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::net::TcpStream;

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

#[derive(Deserialize, Serialize, Clone)]
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

    pub fn rfid_auth_request(checkpoint_id: u32,
                      worker_id: u32) -> CheckpointRequest {
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

    pub fn fingerprint_auth_req(checkpoint_id: u32,
                                worker_id: u32,
                                worker_fingerprint: String) -> CheckpointRequest {
        
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

    pub fn enroll_req(checkpoint_id: u32,
                      worker_name: String,
                      worker_fingerprint: String,
                      location: String,
                      role_id: u32) -> CheckpointRequest {
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

    pub fn update_req(checkpoint_id: u32,
                      worker_id: u32,
                      new_role_id: u32,
                      new_location: String) -> CheckpointRequest {
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
    
    pub fn delete_req(checkpoint_id: u32,
                      worker_id: u32) -> CheckpointRequest {
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
    pub fn auth_reply(state: CheckpointState) -> Self{
        return CheckpointReply {
            status: "success".to_string(),
            checkpoint_id: None,
            worker_id: None,
            fingerprint: None,
            data: None,
            auth_response: Some(state),
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
        }
    }
    pub fn auth_reply(checkpoint_id: u32,
                      worker_id: u32,
                      worker_fingerprint: String,
                      role_id: u32,
                      authorized_roles: String,
                      location: String,
                      allowed_locations: String,
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
        }
    }
}
