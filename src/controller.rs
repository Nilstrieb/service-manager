use crate::model::config::Config;
use crate::model::{AppState, Service, ServiceStatus};
use crate::{view, App};
use crossterm::event;
use crossterm::event::{Event, KeyCode};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io;
use std::process::{Command, Stdio};
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
                KeyCode::Char('r') => app.run_service(),
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
    pub fn new(config: Config) -> App {
        App {
            table: AppState {
                table_state: TableState::default(),
                items: config
                    .into_iter()
                    .map(|(name, service)| Service {
                        command: service.command,
                        name,
                        workdir: service
                            .workdir
                            .unwrap_or_else(|| std::env::current_dir().unwrap()),
                        env: service.env.unwrap_or_else(HashMap::new),
                        status: ServiceStatus::NotStarted,
                        child: None,
                    })
                    .collect(),
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
                self.selected = Some(selected);
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

    fn run_service(&mut self) {
        if let Some(selected) = self.selected {
            self.start_service(selected)
        } else if let Some(selected) = self.table.table_state.selected() {
            self.start_service(selected)
        }
    }

    fn start_service(&mut self, service: usize) {
        let service = &mut self.table.items[service];
        service.status = ServiceStatus::Running;

        let mut cmd = Command::new("sh");

        cmd.args(["-c", &service.command]);
        cmd.envs(service.env.iter());

        cmd.stdout(Stdio::piped());
        let child = cmd.spawn();

        service.child = Some(child);
    }
}
