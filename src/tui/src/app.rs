use color_eyre::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
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
        let stdout = io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        self.running = true;
        while self.running {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_crossterm_events()?;
        }

        disable_raw_mode()?;
        execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;
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

        // Create a layout: header area (Length 5) and the rest for content.
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
