/**********************************
            IMPORTS
**********************************/
use rppal::i2c::I2c;
use serde::{Deserialize, Serialize};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use polynomial_ring::Polynomial;
use rand_distr::{Uniform, Normal, Distribution};
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::Rng;
use std::collections::HashMap;
use openssl::symm::{Cipher, Crypter, Mode};

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

/***************************************
*           Cryptography 
****************************************/
pub struct Parameters {
    pub n: usize,       // Polynomial modulus degree
    pub q: i64,       // Ciphertext modulus
    pub t: i64,       // Plaintext modulus
    pub f: Polynomial<i64>, // Polynomial modulus (x^n + 1 representation)
    pub sigma: f64,    // Standard deviation for normal distribution
}

impl Default for Parameters {
    fn default() -> Self {
        let n = 512;
        let q = 1048576;
        let t = 256;
        let mut poly_vec = vec![0i64;n+1];
        poly_vec[0] = 1;
        poly_vec[n] = 1;
        let f = Polynomial::new(poly_vec);
        let sigma = 8.0;
        Parameters { n, q, t, f, sigma}
    }
}

// ---------- Polynomial Operations ----------
pub fn mod_coeffs(x : Polynomial<i64>, modulus : i64) -> Polynomial<i64> {
	//Take remainder of the coefficients of a polynom by a given modulus
	//Args:
	//	x: polynom
	//	modulus: coefficient modulus
	//Returns:
	//	polynomial in Z_modulus[X]
	let coeffs = x.coeffs();
	let mut newcoeffs = vec![];
	let mut c;
	if coeffs.len() == 0 {
		// return original input for the zero polynomial
		x
	} else {
		for i in 0..coeffs.len() {
			c = coeffs[i].rem_euclid(modulus);
			if c > modulus/2 {
				c = c-modulus;
			}
			newcoeffs.push(c);
		}
		Polynomial::new(newcoeffs)
	}
}

pub fn polymul(x : &Polynomial<i64>, y : &Polynomial<i64>, modulus : i64, f : &Polynomial<i64>) -> Polynomial<i64> {
    //Multiply two polynoms
    //Args:
    //	x, y: two polynoms to be multiplied.
    //	modulus: coefficient modulus.
    //	f: polynomial modulus.
    //Returns:
    //	polynomial in Z_modulus[X]/(f).
	let mut r = x*y;
    r.division(f);
    if modulus != 0 {
        mod_coeffs(r, modulus)
    }
    else{
        r
    }
}

pub fn polyadd(x : &Polynomial<i64>, y : &Polynomial<i64>, modulus : i64, f : &Polynomial<i64>) -> Polynomial<i64> {
    //Add two polynoms
    //Args:
    //	x, y: two polynoms to be added.
    //	modulus: coefficient modulus.
    //	f: polynomial modulus.
    //Returns:
    //	polynomial in Z_modulus[X]/(f).
	let mut r = x+y;
    r.division(f);
    if modulus != 0 {
        mod_coeffs(r, modulus)
    }
    else{
        r
    }
}

pub fn polyinv(x : &Polynomial<i64>, modulus: i64) -> Polynomial<i64> {
    //Additive inverse of polynomial x modulo modulus
    let y = -x;
    if modulus != 0{
      mod_coeffs(y, modulus)
    }
    else {
      y
    }
  }

pub fn polysub(x : &Polynomial<i64>, y : &Polynomial<i64>, modulus : i64, f : Polynomial<i64>) -> Polynomial<i64> {
    //Subtract two polynoms
    //Args:
    //	x, y: two polynoms to be added.
    //	modulus: coefficient modulus.
    //	f: polynomial modulus.
    //Returns:
    //	polynomial in Z_modulus[X]/(f).
	polyadd(x, &polyinv(y, modulus), modulus, &f)
}

// ---------- Polynomial Generators ----------
pub fn gen_binary_poly(size: usize, seed: Option<u64>) -> Polynomial<i64> {
    let between = Uniform::new(0, 2).expect("Failed to create uniform distribution");
    let mut rng = match seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => {
            let mut rng = rand::rng();
            StdRng::from_seed(rng.random::<[u8; 32]>())
        },
    };
    let coeffs: Vec<i64> = (0..size).map(|_| between.sample(&mut rng)).collect();
    Polynomial::new(coeffs)
}

pub fn gen_ternary_poly(size: usize, seed: Option<u64>) -> Polynomial<i64> {
    let between = Uniform::new(-1, 2).expect("Failed to create uniform distribution");
    let mut rng = match seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => {
            let mut rng = rand::rng();
            StdRng::from_seed(rng.random::<[u8; 32]>())
        },
    };
    let coeffs: Vec<i64> = (0..size).map(|_| between.sample(&mut rng)).collect();
    Polynomial::new(coeffs)
}


pub fn gen_uniform_poly(size: usize, q: i64, seed: Option<u64>) -> Polynomial<i64> {
    let between = Uniform::new(0, q).expect("Failed to create uniform distribution");
    let mut rng = match seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => {
            let mut rng = rand::rng();
            StdRng::from_seed(rng.random::<[u8; 32]>())
        },
    };
    let coeffs: Vec<i64> = (0..size).map(|_| between.sample(&mut rng)).collect();
    Polynomial::new(coeffs)
}

pub fn gen_normal_poly(size: usize, sigma: f64, seed: Option<u64>) -> Polynomial<i64> {
    let normal = Normal::new(0.0, sigma).expect("Failed to create normal distribution");
    let mut rng = match seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => {
            let mut rng = rand::rng();
            StdRng::from_seed(rng.random::<[u8; 32]>())
        },
    };
    let coeffs: Vec<i64> = (0..size).map(|_| normal.sample(&mut rng).round() as i64).collect();
    Polynomial::new(coeffs)
}


//returns the nearest integer to a/b
pub fn nearest_int(a: i64, b: i64) -> i64 {
    (a + b / 2) / b
}

// ---------- RLWE Key Generation ----------
pub fn keygen(params: &Parameters, seed: Option<u64>) -> ([Polynomial<i64>; 2], Polynomial<i64>) {

    let (n, q, f) = (params.n, params.q, &params.f);

    //Generate Keys
    let secret = gen_ternary_poly(n, seed);
    let a: Polynomial<i64> = gen_uniform_poly(n, q, seed);
    let error = gen_ternary_poly(n, seed);
    let b = polyadd(&polymul(&polyinv(&a,q*q), &secret, q*q, &f), &polyinv(&error,q*q), q*q, &f);
    

    ([b, a], secret)
}


pub fn keygen_string(params: &Parameters, seed: Option<u64>) -> HashMap<String,String> {

    let (public, secret) = keygen(params,seed);
    let mut pk_coeffs: Vec<i64> = Vec::with_capacity(2*params.n);
    pk_coeffs.extend(public[0].coeffs());
    pk_coeffs.extend(public[1].coeffs());

    let pk_coeffs_str = pk_coeffs.iter()
            .map(|coef| coef.to_string())
            .collect::<Vec<String>>()
            .join(",");
    
    let sk_coeffs_str = secret.coeffs().iter()
            .map(|coef| coef.to_string())
            .collect::<Vec<String>>()
            .join(",");
    
    let mut keys: HashMap<String, String> = HashMap::new();
    keys.insert(String::from("secret"), sk_coeffs_str);
    keys.insert(String::from("public"), pk_coeffs_str);
    keys
}

// ---------- RLWE Encryption ----------
pub fn encrypt(
    public: &[Polynomial<i64>; 2],   
    m: &Polynomial<i64>,       
    params: &Parameters,     
    seed: Option<u64>      
) -> (Polynomial<i64>, Polynomial<i64>) {
    let (n, q, t, f) = (params.n, params.q, params.t, &params.f);
    let scaled_m = mod_coeffs(m * q / t, q);

    let e1 = gen_ternary_poly(n, seed);
    let e2 = gen_ternary_poly(n, seed);
    let u = gen_ternary_poly(n, seed);

    let ct0 = polyadd(&polyadd(&polymul(&public[0], &u, q*q, f), &e1, q*q, f), &scaled_m, q*q, f);
    let ct1 = polyadd(&polymul(&public[1], &u, q*q, f), &e2, q*q, f);

    (ct0, ct1)
}


pub fn encrypt_string(pk_string: &String, message: &String, params: &Parameters, seed: Option<u64>) -> String {
    let pk_arr: Vec<i64> = pk_string
        .split(',')
        .filter_map(|x| x.parse::<i64>().ok())
        .collect();

    let pk_b = Polynomial::new(pk_arr[..params.n].to_vec());
    let pk_a = Polynomial::new(pk_arr[params.n..].to_vec());
    let pk = [pk_b, pk_a];

    let message_bytes: Vec<u8> = message.as_bytes().to_vec();
    let message_ints: Vec<i64> = message_bytes.iter().map(|&byte| byte as i64).collect();
    let message_poly = Polynomial::new(message_ints);

    
    let ciphertext = encrypt(&pk, &message_poly, params, seed);

    
    let ciphertext_string = ciphertext.0.coeffs()
        .iter()
        .chain(ciphertext.1.coeffs().iter())
        .map(|x| x.to_string())
        .collect::<Vec<String>>()
        .join(",");

    ciphertext_string
}

// ---------- AES Encrypt ----------
pub fn encrypt_aes_string(aes_key: &[u8; 32], iv: &[u8; 16], plaintext: &str) -> String {
    let cipher = Cipher::aes_256_cbc();
    let mut crypter = Crypter::new(cipher, Mode::Encrypt, aes_key, Some(iv))
        .expect("Failed to create encryptor");

    let mut ciphertext = vec![0; plaintext.len() + cipher.block_size()];
    let mut count = crypter.update(plaintext.as_bytes(), &mut ciphertext)
        .expect("Encryption failed");
    count += crypter.finalize(&mut ciphertext[count..])
        .expect("Finalization failed");

    ciphertext.truncate(count);
    hex::encode(&ciphertext)
}



// ---------- RLWE Decryption ----------
pub fn decrypt(
    secret: &Polynomial<i64>,   
    cipher: &[Polynomial<i64>; 2],        
    params: &Parameters
) -> Polynomial<i64> {
    let (_n, q, t, f) = (params.n, params.q, params.t, &params.f);
    let scaled_pt = polyadd(&polymul(&cipher[1], secret, q, f), &cipher[0], q, f);
    
    let mut decrypted_coeffs = vec![];
    for c in scaled_pt.coeffs().iter() {
        let s = nearest_int(c * t, q);
        decrypted_coeffs.push(s.rem_euclid(t));
    }
    
    Polynomial::new(decrypted_coeffs)
}

pub fn decrypt_string(sk_string: &String, ciphertext_string: &String, params: &Parameters) -> String {
    let sk_coeffs: Vec<i64> = sk_string
        .split(',')
        .filter_map(|x| x.parse::<i64>().ok())
        .collect();
    let sk = Polynomial::new(sk_coeffs);

    let ciphertext_array: Vec<i64> = ciphertext_string
        .split(',')
        .map(|s| s.parse::<i64>().unwrap())
        .collect();

    let num_bytes = ciphertext_array.len() / (2 * params.n);
    let mut decrypted_message = String::new();

    for i in 0..num_bytes {
        let c0 = Polynomial::new(ciphertext_array[2 * i * params.n..(2 * i + 1) * params.n].to_vec());
        let c1 = Polynomial::new(ciphertext_array[(2 * i + 1) * params.n..(2 * i + 2) * params.n].to_vec());
        let ct = [c0, c1];

        let decrypted_poly = decrypt(&sk, &ct, &params);

        decrypted_message.push_str(
            &decrypted_poly
                .coeffs()
                .iter()
                .map(|&coeff| coeff as u8 as char)
                .collect::<String>(),
        );
    }

    decrypted_message.trim_end_matches('\0').to_string()
}


// ---------- AES Decryption ----------
pub fn decrypt_aes_string(aes_key: &[u8; 32], iv: &[u8; 16], ciphertext_hex: &str) -> String {
    let cipher = Cipher::aes_256_cbc();
    let ciphertext = hex::decode(ciphertext_hex).expect("Invalid hex encoding");

    let mut decrypter = Crypter::new(cipher, Mode::Decrypt, aes_key, Some(iv))
        .expect("Failed to create decryptor");

    let mut decrypted_bytes = vec![0; ciphertext.len() + cipher.block_size()];
    let mut count = decrypter.update(&ciphertext, &mut decrypted_bytes)
        .expect("Decryption failed");
    count += decrypter.finalize(&mut decrypted_bytes[count..])
        .expect("Finalization failed");

    decrypted_bytes.truncate(count);
    String::from_utf8(decrypted_bytes).expect("Decrypted text is not valid UTF-8")
}



// ---------- Generate IV and Key ----------
pub fn generate_iv() -> [u8; 16] {
    let mut rng = rand::rng();
    rng.random::<[u8; 16]>()
}

pub fn generate_key() -> [u8; 32] {
    let mut rng = rand::rng();
    rng.random::<[u8; 32]>()
}
