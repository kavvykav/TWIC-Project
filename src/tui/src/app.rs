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
enum AppMode {
    Main,
    Submenu {
        main_index: usize,     // Which main menu option’s submenu is active.
        selected_index: usize, // Which submenu item is selected.
    },
}

#[derive(Debug)]
pub struct App {
    running: bool,
    // Index for the currently selected main menu option.
    selected_index: usize,
    // The current mode determines what is rendered.
    mode: AppMode,
    // The main menu items.
    menu_items: Vec<&'static str>,
    // A vector of submenus. Each submenu corresponds to a main menu option.
    submenus: Vec<Vec<&'static str>>,
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
            submenus: vec![
                vec!["Status Details", "Statistics", "Logs"],
                vec!["Edit Config", "Reset Port", "Backup Config"],
                vec!["Live Traffic", "Historical Data", "Alerts"],
            ],
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn run(mut self) -> Result<()> {
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
        Ok(())
    }

    /// Draws the UI.
    /// A header is rendered at the top with enough height to show 3 lines of text.
    /// The menu (or submenu) occupies the rest of the screen.
    fn draw(&mut self, frame: &mut Frame) {
        // Header text varies depending on the current mode.
        let header_text = match self.mode {
            AppMode::Main => {
                "Port Admin Dashboard\nNavigate with arrow keys or hjkl. Enter to select an option.\nPress q, Esc, or Ctrl+C to quit."
                    .to_string()
            }
            AppMode::Submenu { main_index, .. } => {
                format!(
                    "Submenu for {}\nUse arrow keys or hjkl to navigate. Enter to select an option, Esc to go back.\nPress q or Ctrl+C to quit.",
                    self.menu_items[main_index]
                )
            }
        };

        // Increase the header height to 5 to allow the block border to render 3 lines of text.
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Length(5), Constraint::Min(0)].as_ref())
            .split(frame.area());

        let header_paragraph = Paragraph::new(header_text)
            .block(Block::bordered().title("Header"))
            .centered();
        frame.render_widget(header_paragraph, chunks[0]);

        // Render the main menu or the submenu, depending on the current mode.
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
            AppMode::Submenu {
                main_index,
                selected_index,
            } => {
                let submenu = &self.submenus[*main_index];
                let submenu_items: Vec<ListItem> = submenu
                    .iter()
                    .enumerate()
                    .map(|(i, &item)| {
                        let style = if i == *selected_index {
                            Style::default().add_modifier(Modifier::REVERSED)
                        } else {
                            Style::default()
                        };
                        ListItem::new(item).style(style)
                    })
                    .collect();
                let submenu_widget = List::new(submenu_items)
                    .block(Block::bordered().title("Submenu (Esc: back, q, Ctrl+C: quit)"));
                frame.render_widget(submenu_widget, chunks[1]);
            }
        }
    }

    /// Reads crossterm events and updates the state accordingly.
    fn handle_crossterm_events(&mut self) -> Result<()> {
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key_event(key),
            Event::Mouse(_) => {}
            Event::Resize(_, _) => {}
            _ => {}
        }
        Ok(())
    }

    /// Handles key events for both the main menu and submenu.
    fn on_key_event(&mut self, key: KeyEvent) {
        // Global quit keys: q or Ctrl+C always quit.
        if let KeyCode::Char('q') = key.code {
            self.quit();
            return;
        }
        if key.modifiers == KeyModifiers::CONTROL
            && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('C'))
        {
            self.quit();
            return;
        }

        match &mut self.mode {
            AppMode::Main => {
                // In the main menu, Esc also quits.
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
                    // Enter opens the submenu for the selected main option.
                    KeyCode::Enter => {
                        self.mode = AppMode::Submenu {
                            main_index: self.selected_index,
                            selected_index: 0,
                        };
                    }
                    _ => {}
                }
            }
            AppMode::Submenu {
                main_index,
                selected_index,
            } => {
                // In a submenu, Esc returns to the main menu.
                if key.code == KeyCode::Esc {
                    self.mode = AppMode::Main;
                    return;
                }
                let submenu_len = self.submenus[*main_index].len();
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if *selected_index > 0 {
                            *selected_index -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if *selected_index < submenu_len - 1 {
                            *selected_index += 1;
                        }
                    }
                    // Enter triggers a placeholder action then returns to the main menu.
                    KeyCode::Enter => {
                        println!(
                            "Selected submenu option: '{}' for main option: '{}'",
                            self.submenus[*main_index][*selected_index],
                            self.menu_items[*main_index]
                        );
                        self.mode = AppMode::Main;
                    }
                    _ => {}
                }
            }
        }
    }

    fn quit(&mut self) {
        self.running = false;
    }
}
