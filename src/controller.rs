use crate::model::{AppState, AppStateFullView};
use crate::{view, App};
use crossterm::event;
use crossterm::event::{Event, KeyCode};
use std::io;
use tui::backend::Backend;
use tui::widgets::TableState;
use tui::Terminal;

pub fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> io::Result<()> {
    loop {
        terminal.draw(|f| view::render_ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => match app.selected {
                    Some(_) => app.leave_service(),
                    None => return Ok(()),
                },
                KeyCode::Down => app.next(),
                KeyCode::Up => app.previous(),
                KeyCode::Enter => app.select_service(),
                KeyCode::Esc => app.leave_service(),
                _ => {}
            }
        }
    }
}

impl App {
    pub fn new() -> App {
        App {
            table: AppState {
                table_state: TableState::default(),
                items: vec![
                    vec!["backend".to_string(), "running".to_string()],
                    vec!["frontend".to_string(), "exited (0)".to_string()],
                    vec!["database".to_string(), "failed (1)".to_string()],
                ],
            },
            selected: None,
        }
    }

    pub fn is_table(&self) -> bool {
        self.selected.is_none()
    }

    fn select_service(&mut self) {
        if self.is_table() {
            if let Some(selected) = self.table.table_state.selected() {
                self.selected = Some(AppStateFullView {
                    name: self.table.items[selected][0].to_string(),
                });
            }
        }
    }

    fn leave_service(&mut self) {
        self.selected = None;
    }

    fn next(&mut self) {
        let i = match self.table.table_state.selected() {
            Some(i) => {
                if i >= self.table.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.table.table_state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = match self.table.table_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.table.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.table.table_state.select(Some(i));
    }
}
