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
use base64::{encode, decode};

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

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CheckpointReply {
    pub status: String,
    pub checkpoint_id: Option<u32>,
    pub worker_id: Option<u32>,
    pub fingerprint: Option<String>,
    pub data: Option<String>,
    pub auth_response: Option<CheckpointState>,
    pub rfid_ver: Option<bool>,
}

#[derive(Serialize, Clone, Deserialize, Debug)]
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
            rfid_ver: Some(false),
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
            rfid_ver: Some(true),
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
            rfid_ver: None,
        };
    }
}

/*********************************************
    PORT SERVER <--> CENTRAL DATABASE
*********************************************/
pub const SERVER_ADDR: &str = "127.0.0.1:8080";
pub const DATABASE_ADDR: &str = "127.0.0.1:3036";

// Client structure for a port server to manage checkpoints
#[derive(Clone, Debug)]
pub struct Client {
    pub id: usize,
    pub stream: Arc<Mutex<TcpStream>>,
    pub state: CheckpointState,
}

// Format for requests to the Database
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct DatabaseRequest {
    pub command: String,
    pub checkpoint_id: Option<u32>,
    pub worker_id: Option<u32>,
    pub worker_name: Option<String>,
    pub worker_fingerprint: Option<String>,
    pub location: Option<String>,
    pub authorized_roles: Option<String>,
    pub role_id: Option<u32>,
    pub encrypted_aes_key: Option<String>,
    pub encrypted_iv: Option<String>,
    pub public_key: Option<String>,
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
    pub encrypted_aes_key: Option<String>,
    pub encrypted_iv: Option<String>,
    pub public_key: Option<String>,
}

impl DatabaseReply {
    pub fn success(worker_id: u32) -> Self {
        DatabaseReply {
            status: "success".to_string(),
            checkpoint_id: None,
            worker_id: Some(worker_id),
            worker_fingerprint: None,
            role_id: None,
            authorized_roles: None,
            location: None,
            auth_response: None,
            allowed_locations: None,
            worker_name: None,
            encrypted_aes_key: None,
            encrypted_iv: None,
            public_key: None,
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
            encrypted_aes_key: None,
            encrypted_iv: None,
            public_key: None,
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
            encrypted_aes_key: None,
            encrypted_iv: None,
            public_key: None,
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
            encrypted_aes_key: None,
            encrypted_iv: None,
            public_key: None,
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
            encrypted_aes_key: None,
            encrypted_iv: None,
            public_key: None,
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
use color_eyre::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    prelude::*,
    style::{Modifier, Style},
    widgets::{Block, List, ListItem, Paragraph},
    Terminal,
};
use std::io;

#[derive(Debug)]
pub enum Submission {
    Enroll {
        name: String,
        biometric: String,
        role_id: String,
        location: String,
    },
    Update {
        employee_id: String,
        role_id: String,
    },
    Delete {
        employee_id: String,
    },
}

#[derive(Debug)]
enum AppMode {
    Main,
    EnrollForm {
        name: String,
        biometric: String,
        role_id: String,
        location: String,
        active_field: usize, // 0: Name, 1: Biometric, 2: Role ID, 3: Location
        editing: bool,       // false: navigation mode; true: editing mode
    },
    UpdateForm {
        employee_id: String,
        role_id: String,
        active_field: usize, // 0: Employee ID, 1: Role ID
        editing: bool,
    },
    DeleteForm {
        employee_id: String,
        editing: bool,
    },
}

pub struct App {
    running: bool,
    // Main menu selection index.
    selected_index: usize,
    // Current mode determines what is rendered.
    mode: AppMode,
    // Main menu items.
    menu_items: Vec<&'static str>,
    // When a form is submitted, this is set.
    submission: Option<Submission>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            running: false,
            selected_index: 0,
            mode: AppMode::Main,
            menu_items: vec![
                "Enroll new employee",
                "Update existing employee",
                "Delete existing employee",
            ],
            submission: None,
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs the TUI app. When a form is submitted, the corresponding submission
    /// is stored and the TUI quits. This method then returns the submission (if any).
    pub fn run(mut self) -> Result<Option<Submission>> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        // Enter the alternate screen so the TUI uses a separate buffer.
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        self.running = true;
        while self.running {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_crossterm_events()?;
        }

        disable_raw_mode()?;
        // Leave the alternate screen to restore the original terminal.
        execute!(io::stdout(), LeaveAlternateScreen)?;
        Ok(self.submission)
    }

    fn draw(&mut self, frame: &mut Frame) {
        let header_text = match &self.mode {
            AppMode::Main => {
                "Employee Management Dashboard\nUse arrow keys or j/k to navigate. Enter to select/activate a field.\nPress Ctrl+S to submit a form, Esc to cancel, q or Ctrl+C to quit."
                    .to_string()
            }
            AppMode::EnrollForm { .. } => {
                "Enroll New Employee\nPress Enter on a field to start/stop editing (j/k won’t navigate while editing).\nPress Ctrl+S to submit, Esc to cancel."
                    .to_string()
            }
            AppMode::UpdateForm { .. } => {
                "Update Employee\nPress Enter on a field to start/stop editing (j/k won’t navigate while editing).\nPress Ctrl+S to submit, Esc to cancel."
                    .to_string()
            }
            AppMode::DeleteForm { .. } => {
                "Delete Employee\nPress Enter to start/stop editing the Employee ID.\nPress Ctrl+S to submit, Esc to cancel."
                    .to_string()
            }
        };

        // Allocate a header area (Length 5) and the rest for content.
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Length(5), Constraint::Min(0)].as_ref())
            .split(frame.area());

        let header_paragraph = Paragraph::new(header_text)
            .block(Block::bordered().title("Header"))
            .centered();
        frame.render_widget(header_paragraph, chunks[0]);

        match &self.mode {
            AppMode::Main => {
                let main_menu_items: Vec<ListItem> = self
                    .menu_items
                    .iter()
                    .enumerate()
                    .map(|(i, &item)| {
                        let style = if i == self.selected_index {
                            Style::default().add_modifier(Modifier::REVERSED)
                        } else {
                            Style::default()
                        };
                        ListItem::new(item).style(style)
                    })
                    .collect();
                let main_menu = List::new(main_menu_items)
                    .block(Block::bordered().title("Main Menu (q, Esc, Ctrl+C: quit)"));
                frame.render_widget(main_menu, chunks[1]);
            }
            AppMode::EnrollForm {
                name,
                biometric,
                role_id,
                location,
                active_field,
                editing,
            } => {
                let fields = vec![
                    format!("Name: {}", name),
                    format!("Biometric: {}", biometric),
                    format!("Role ID: {}", role_id),
                    format!("Location: {}", location),
                ];
                let list_items: Vec<ListItem> = fields
                    .into_iter()
                    .enumerate()
                    .map(|(i, field)| {
                        let mut style = if i == *active_field {
                            Style::default().add_modifier(Modifier::REVERSED)
                        } else {
                            Style::default()
                        };
                        if i == *active_field && *editing {
                            style = style.add_modifier(Modifier::UNDERLINED);
                        }
                        ListItem::new(field).style(style)
                    })
                    .collect();
                let form_list =
                    List::new(list_items).block(Block::bordered().title(
                        "Enroll New Employee (Enter: edit field, Ctrl+S: submit, Esc: cancel)",
                    ));
                frame.render_widget(form_list, chunks[1]);
            }
            AppMode::UpdateForm {
                employee_id,
                role_id,
                active_field,
                editing,
            } => {
                let fields = vec![
                    format!("Employee ID: {}", employee_id),
                    format!("Role ID: {}", role_id),
                ];
                let list_items: Vec<ListItem> = fields
                    .into_iter()
                    .enumerate()
                    .map(|(i, field)| {
                        let mut style = if i == *active_field {
                            Style::default().add_modifier(Modifier::REVERSED)
                        } else {
                            Style::default()
                        };
                        if i == *active_field && *editing {
                            style = style.add_modifier(Modifier::UNDERLINED);
                        }
                        ListItem::new(field).style(style)
                    })
                    .collect();
                let form_list = List::new(list_items).block(
                    Block::bordered()
                        .title("Update Employee (Enter: edit field, Ctrl+S: submit, Esc: cancel)"),
                );
                frame.render_widget(form_list, chunks[1]);
            }
            AppMode::DeleteForm {
                employee_id,
                editing,
            } => {
                let field = format!("Employee ID: {}", employee_id);
                let mut style = Style::default();
                if *editing {
                    style = style
                        .add_modifier(Modifier::REVERSED)
                        .add_modifier(Modifier::UNDERLINED);
                }
                let list_item = ListItem::new(field).style(style);
                let form_list = List::new(vec![list_item]).block(
                    Block::bordered()
                        .title("Delete Employee (Enter: edit field, Ctrl+S: submit, Esc: cancel)"),
                );
                frame.render_widget(form_list, chunks[1]);
            }
        }
    }

    fn handle_crossterm_events(&mut self) -> Result<()> {
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key_event(key),
            Event::Mouse(_) => {}
            Event::Resize(_, _) => {}
            _ => {}
        }
        Ok(())
    }

    fn on_key_event(&mut self, key: KeyEvent) {
        // Global quit keys.
        if let KeyCode::Char('q') = key.code {
            self.quit();
            return;
        }
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
            self.quit();
            return;
        }

        match &mut self.mode {
            AppMode::Main => {
                if key.code == KeyCode::Esc {
                    self.quit();
                    return;
                }
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if self.selected_index > 0 {
                            self.selected_index -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.selected_index < self.menu_items.len() - 1 {
                            self.selected_index += 1;
                        }
                    }
                    KeyCode::Enter => match self.selected_index {
                        0 => {
                            self.mode = AppMode::EnrollForm {
                                name: String::new(),
                                biometric: String::new(),
                                role_id: String::new(),
                                location: String::new(),
                                active_field: 0,
                                editing: false,
                            };
                        }
                        1 => {
                            self.mode = AppMode::UpdateForm {
                                employee_id: String::new(),
                                role_id: String::new(),
                                active_field: 0,
                                editing: false,
                            };
                        }
                        2 => {
                            self.mode = AppMode::DeleteForm {
                                employee_id: String::new(),
                                editing: false,
                            };
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            AppMode::EnrollForm {
                name,
                biometric,
                role_id,
                location,
                active_field,
                editing,
            } => {
                if *editing {
                    match key.code {
                        KeyCode::Enter => {
                            *editing = false;
                        }
                        KeyCode::Backspace => match *active_field {
                            0 => {
                                name.pop();
                            }
                            1 => {
                                biometric.pop();
                            }
                            2 => {
                                role_id.pop();
                            }
                            3 => {
                                location.pop();
                            }
                            _ => {}
                        },
                        KeyCode::Char(c) => match *active_field {
                            0 => {
                                name.push(c);
                            }
                            1 => {
                                biometric.push(c);
                            }
                            2 => {
                                role_id.push(c);
                            }
                            3 => {
                                location.push(c);
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                } else {
                    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('s') {
                        self.submission = Some(Submission::Enroll {
                            name: name.clone(),
                            biometric: biometric.clone(),
                            role_id: role_id.clone(),
                            location: location.clone(),
                        });
                        self.quit();
                        return;
                    }
                    match key.code {
                        KeyCode::Enter => {
                            *editing = true;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if *active_field > 0 {
                                *active_field -= 1;
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if *active_field < 3 {
                                *active_field += 1;
                            }
                        }
                        KeyCode::Tab => {
                            *active_field = (*active_field + 1) % 4;
                        }
                        KeyCode::Esc => {
                            self.mode = AppMode::Main;
                        }
                        _ => {}
                    }
                }
            }
            AppMode::UpdateForm {
                employee_id,
                role_id,
                active_field,
                editing,
            } => {
                if *editing {
                    match key.code {
                        KeyCode::Enter => {
                            *editing = false;
                        }
                        KeyCode::Backspace => match *active_field {
                            0 => {
                                employee_id.pop();
                            }
                            1 => {
                                role_id.pop();
                            }
                            _ => {}
                        },
                        KeyCode::Char(c) => match *active_field {
                            0 => {
                                employee_id.push(c);
                            }
                            1 => {
                                role_id.push(c);
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                } else {
                    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('s') {
                        self.submission = Some(Submission::Update {
                            employee_id: employee_id.clone(),
                            role_id: role_id.clone(),
                        });
                        self.quit();
                        return;
                    }
                    match key.code {
                        KeyCode::Enter => {
                            *editing = true;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if *active_field > 0 {
                                *active_field -= 1;
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if *active_field < 1 {
                                *active_field += 1;
                            }
                        }
                        KeyCode::Tab => {
                            *active_field = (*active_field + 1) % 2;
                        }
                        KeyCode::Esc => {
                            self.mode = AppMode::Main;
                        }
                        _ => {}
                    }
                }
            }
            AppMode::DeleteForm {
                employee_id,
                editing,
            } => {
                if *editing {
                    match key.code {
                        KeyCode::Enter => {
                            *editing = false;
                        }
                        KeyCode::Backspace => {
                            employee_id.pop();
                        }
                        KeyCode::Char(c) => {
                            employee_id.push(c);
                        }
                        _ => {}
                    }
                } else {
                    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('s') {
                        self.submission = Some(Submission::Delete {
                            employee_id: employee_id.clone(),
                        });
                        self.quit();
                        return;
                    }
                    match key.code {
                        KeyCode::Enter => {
                            *editing = true;
                        }
                        KeyCode::Esc => {
                            self.mode = AppMode::Main;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn quit(&mut self) {
        self.running = false;
    }
}


/***************************************
*           Cryptography 
****************************************/

#[derive(Debug)]
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

pub fn encrypt_string(pk_string: &String, message: &[u8], params: &Parameters, seed: Option<u64>) -> String {
    let message_str = encode(message); // Convert u8 array to Base64 String
    let pk_arr: Vec<i64> = pk_string
        .split(',')
        .filter_map(|x| x.parse::<i64>().ok())
        .collect();

    let pk_b = Polynomial::new(pk_arr[..params.n].to_vec());
    let pk_a = Polynomial::new(pk_arr[params.n..].to_vec());
    let pk = [pk_b, pk_a];

    let message_bytes: Vec<u8> = message_str.as_bytes().to_vec();
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
pub fn encrypt_aes(plaintext: &str, key: &[u8], iv: &[u8]) -> Vec<u8> {
    let cipher = Cipher::aes_256_cbc();
    let mut encrypter = Crypter::new(cipher, Mode::Encrypt, key, Some(iv)).expect("Failed to create encrypter");
    encrypter.pad(true);

    let mut ciphertext = vec![0; plaintext.len() + cipher.block_size()];
    let mut count = encrypter.update(plaintext.as_bytes(), &mut ciphertext).expect("Encryption failed");
    count += encrypter.finalize(&mut ciphertext[count..]).expect("Final encryption step failed");

    ciphertext.truncate(count);
    ciphertext
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


pub fn decrypt_string(sk_string: &String, ciphertext_string: &String, params: &Parameters) -> Vec<u8> {
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

    let decoded_bytes = decode(decrypted_message.trim_end_matches('\0')).expect("Failed to decode Base64");
    decoded_bytes
}


// ---------- AES Decryption ----------
pub fn decrypt_aes(ciphertext: &[u8], key: &[u8], iv: &[u8]) -> String {
    let cipher = Cipher::aes_256_cbc();
    let mut decrypter = Crypter::new(cipher, Mode::Decrypt, key, Some(iv)).expect("Failed to create decrypter");
    decrypter.pad(true);

    let mut plaintext = vec![0; ciphertext.len() + cipher.block_size()];
    let mut count = decrypter.update(ciphertext, &mut plaintext).expect("Decryption failed");
    count += decrypter.finalize(&mut plaintext[count..]).expect("Final decryption step failed");

    plaintext.truncate(count);
    String::from_utf8(plaintext).expect("Invalid UTF-8")
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
