// src/app.rs
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
    text::Line,
    widgets::{Block, List, ListItem, Paragraph},
    Terminal,
};
use std::io;

#[derive(Debug, Default)]
pub struct App {
    running: bool,
    selected_index: usize,
    menu_items: Vec<&'static str>,
}

impl App {
    pub fn new() -> Self {
        Self {
            running: false,
            selected_index: 0,
            menu_items: vec!["View Log", "Employee Modification"],
        }
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

    // Removed the generic parameter from Frame.
    fn draw(&mut self, frame: &mut Frame) {
        let title = Line::from("Port Admin Dashboard").bold().blue().centered();
        let text = "Navigate with arrow keys or hjkl. Select an option.\n\
            Press `Esc`, `Ctrl-C` or `q` to stop running.";

        let paragraph = Paragraph::new(text)
            .block(Block::bordered().title(title))
            .centered();

        let menu_items: Vec<ListItem> = self
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
        let menu = List::new(menu_items).block(Block::bordered().title("Actions"));

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(frame.area());

        frame.render_widget(paragraph, chunks[0]);
        frame.render_widget(menu, chunks[1]);
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
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q'))
            | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => self.quit(),
            (_, KeyCode::Up | KeyCode::Char('k')) => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }
            (_, KeyCode::Down | KeyCode::Char('j')) => {
                if self.selected_index < self.menu_items.len() - 1 {
                    self.selected_index += 1;
                }
            }
            _ => {}
        }
    }

    fn quit(&mut self) {
        self.running = false;
    }
}
