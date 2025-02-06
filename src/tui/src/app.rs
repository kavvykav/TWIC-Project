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
        main_index: usize,     // Which main menu option's submenu is active
        selected_index: usize, // Which submenu item is selected
    },
}

#[derive(Debug)]
pub struct App {
    running: bool,
    // This is used only in Main mode.
    selected_index: usize,
    // Our current state determines what is rendered.
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
    /// A header is rendered in a fixed-height area at the top,
    /// and below that the menu (or submenu) takes up the rest of the space.
    fn draw(&mut self, frame: &mut Frame) {
        // Set header text based on whether we're in the main menu or a submenu.
        let header_text = match self.mode {
            AppMode::Main => {
                "Port Admin Dashboard\nNavigate with arrow keys or hjkl. Enter to select an option. Esc, Ctrl+C or q to quit."
                    .to_string()
            }
            AppMode::Submenu { main_index, .. } => {
                format!(
                    "Submenu for {}\nUse arrow keys or hjkl to navigate. Enter to select an option, Esc to go back.",
                    self.menu_items[main_index]
                )
            }
        };

        // Create a layout with a fixed-height header and the remaining space for the menu.
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
            .split(frame.area());

        let header_paragraph = Paragraph::new(header_text)
            .block(Block::bordered().title("Header"))
            .centered();
        frame.render_widget(header_paragraph, chunks[0]);

        // Depending on the current mode, render either the main menu or the active submenu.
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
                let main_menu =
                    List::new(main_menu_items).block(Block::bordered().title("Main Menu"));
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
                let submenu_widget =
                    List::new(submenu_items).block(Block::bordered().title("Submenu"));
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
        match &mut self.mode {
            AppMode::Main => {
                match (key.modifiers, key.code) {
                    // Quit if Esc, 'q', or Ctrl+C is pressed.
                    (_, KeyCode::Esc | KeyCode::Char('q'))
                    | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => {
                        self.quit()
                    }
                    // Navigate up in the main menu.
                    (_, KeyCode::Up | KeyCode::Char('k')) => {
                        if self.selected_index > 0 {
                            self.selected_index -= 1;
                        }
                    }
                    // Navigate down in the main menu.
                    (_, KeyCode::Down | KeyCode::Char('j')) => {
                        if self.selected_index < self.menu_items.len() - 1 {
                            self.selected_index += 1;
                        }
                    }
                    // Enter opens the submenu for the selected main option.
                    (_, KeyCode::Enter) => {
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
                let submenu_len = self.submenus[*main_index].len();
                match (key.modifiers, key.code) {
                    // Esc returns to the main menu.
                    (_, KeyCode::Esc) => {
                        self.mode = AppMode::Main;
                    }
                    // Navigate up in the submenu.
                    (_, KeyCode::Up | KeyCode::Char('k')) => {
                        if *selected_index > 0 {
                            *selected_index -= 1;
                        }
                    }
                    // Navigate down in the submenu.
                    (_, KeyCode::Down | KeyCode::Char('j')) => {
                        if *selected_index < submenu_len - 1 {
                            *selected_index += 1;
                        }
                    }
                    // Enter triggers a placeholder action then returns to the main menu.
                    (_, KeyCode::Enter) => {
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
