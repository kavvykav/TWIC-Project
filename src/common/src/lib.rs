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

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CheckpointReply {
    pub status: String,
    pub checkpoint_id: Option<u32>,
    pub worker_id: Option<u32>,
    pub fingerprint: Option<String>,
    pub data: Option<String>,
    pub auth_response: Option<CheckpointState>,
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
