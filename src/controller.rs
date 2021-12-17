use crate::model::{AppState, AppStateTable};
use crate::{view, App};
use crossterm::event;
use crossterm::event::{Event, KeyCode};
use std::io;
use tui::backend::Backend;
use tui::widgets::TableState;
use tui::Terminal;

pub fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> io::Result<()> {
    loop {
        terminal.draw(|f| view::ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Down => app.next(),
                KeyCode::Up => app.previous(),
                KeyCode::Enter => app.select(),
                _ => {}
            }
        }
    }
}

impl App {
    pub fn new() -> App {
        App {
            state: AppState::Table(AppStateTable {
                table_state: TableState::default(),
                items: vec![
                    vec!["backend".to_string(), "running".to_string()],
                    vec!["frontend".to_string(), "exited (0)".to_string()],
                    vec!["frontend".to_string(), "failed (1)".to_string()],
                ],
            }),
        }
    }
    pub fn next(&mut self) {
        if let AppState::Table(AppStateTable { table_state, items }) = &mut self.state {
            let i = match table_state.selected() {
                Some(i) => {
                    if i >= items.len() - 1 {
                        0
                    } else {
                        i + 1
                    }
                }
                None => 0,
            };
            table_state.select(Some(i));
        }
    }

    pub fn previous(&mut self) {
        if let AppState::Table(AppStateTable { table_state, items }) = &mut self.state {
            let i = match table_state.selected() {
                Some(i) => {
                    if i == 0 {
                        items.len() - 1
                    } else {
                        i - 1
                    }
                }
                None => 0,
            };
            table_state.select(Some(i));
        }
    }

    pub fn select(&mut self) {}
}
