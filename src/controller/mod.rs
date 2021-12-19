mod child;

use crate::controller::child::child_process_thread;
use crate::model::config::Config;
use crate::model::{AppState, Service, ServiceStatus, SmError, SmResult, StdIoStream};
use crate::{view, App};
use crossterm::event;
use crossterm::event::{Event, KeyCode};
use std::collections::HashMap;
use std::io::{ErrorKind, Write};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use std::{io, thread};
use tracing::{error, info};
use tui::backend::Backend;
use tui::widgets::TableState;
use tui::Terminal;

const STDIO_SEND_BUF_SIZE: usize = 512;

pub type StdioSendBuf = ([u8; STDIO_SEND_BUF_SIZE], usize);

pub fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> SmResult {
    info!("Entering main loop");

    loop {
        terminal.draw(|f| view::render_ui(f, &mut app))?;

        if event::poll(Duration::from_secs(0))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => match app.selected {
                        Some(_) => app.leave_service(),
                        None => {
                            break;
                        }
                    },
                    KeyCode::Char('r') => app.run_service()?,
                    KeyCode::Char('k') => app.kill_service()?,
                    KeyCode::Down => match app.selected {
                        Some(_) => app.scroll_down(),
                        None => app.next(),
                    },
                    KeyCode::Up => match app.selected {
                        Some(_) => app.scroll_up(),
                        None => app.previous(),
                    },
                    KeyCode::Enter => app.select_service(),
                    KeyCode::Esc => app.leave_service(),
                    _ => {}
                }
            }
        }

        app.recv_stdouts(Duration::from_millis(10));
    }

    // terminate the child processes
    for (i, sender) in app.thread_terminates.values().enumerate() {
        info!(index = i, "Terminating child thread...");

        let _ = sender.send(());
    }

    Ok(())
}

impl App {
    pub fn new(config: Config) -> io::Result<App> {
        Ok(App {
            table: AppState {
                table_state: TableState::default(),
                services: config
                    .into_iter()
                    .map(|(name, service)| -> io::Result<Service> {
                        let (stdout_send, stdout_recv) = mpsc::channel();

                        Ok(Service {
                            command: service.command,
                            name,
                            workdir: service
                                .workdir
                                .ok_or_else(|| io::Error::from(ErrorKind::Other))
                                .or_else(|_| std::env::current_dir())?,
                            env: service.env.unwrap_or_else(HashMap::new),
                            status: Arc::new(Mutex::new(ServiceStatus::NotStarted)),
                            std_io_buf: Vec::new(),
                            std_io_line_cache: Vec::new(),
                            stdout: StdIoStream {
                                recv: stdout_recv,
                                send: stdout_send,
                            },
                        })
                    })
                    .collect::<io::Result<_>>()?,
            },
            selected: None,
            scroll_pos: None,
            thread_terminates: HashMap::new(),
        })
    }

    pub fn is_table(&self) -> bool {
        self.selected.is_none()
    }

    fn recv_stdouts(&mut self, duration: Duration) {
        for service in self.table.services.iter_mut() {
            let start = Instant::now();
            let buf = &mut service.std_io_buf;

            while let Ok((new_buf, n)) = service.stdout.recv.try_recv() {
                let len_before = buf.len();
                buf.extend(&new_buf[0..n]);

                let new_newline_positions = new_buf
                    .iter()
                    .enumerate()
                    .filter(|(_, &byte)| byte == b'\n')
                    .map(|(position, _)| position + len_before);

                service.std_io_line_cache.extend(new_newline_positions);

                if buf.len() > 2_000_000 {
                    buf.clear(); // todo don't
                    service.std_io_line_cache.clear();
                }

                if start.elapsed() > duration {
                    break;
                }
            }
        }
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
                if i >= self.table.services.len() - 1 {
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
                    self.table.services.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.table.table_state.select(Some(i));
    }

    fn scroll_up(&mut self) {
        self.scroll_pos = self.scroll_pos.map(|pos| pos + 1);
    }

    fn scroll_down(&mut self) {
        self.scroll_pos = self.scroll_pos.map(|pos| pos - 1);
    }

    fn run_service(&mut self) -> SmResult {
        let index = self.selected.or_else(|| self.table.table_state.selected());

        if let Some(index) = index {
            let status = {
                let service = &mut self.table.services[index];
                service.std_io_buf.clear();
                *service.status.lock()?
            };

            if status != ServiceStatus::Running {
                self.start_service(index)?;
            }
        }

        Ok(())
    }

    fn kill_service(&mut self) -> SmResult {
        let index = self.selected.or_else(|| self.table.table_state.selected());

        if let Some(index) = index {
            let service = &mut self.table.services[index];

            let status = { *service.status.lock()? };

            if status == ServiceStatus::Running {
                info!(name = %service.name,"Killing service");

                let terminate_sender = &mut self
                    .thread_terminates
                    .get(&index)
                    .ok_or(SmError::Bug("Child termination channel not found"))?;
                terminate_sender.send(()).map_err(|_| {
                    SmError::Bug("Failed to send termination signal to child process")
                })?;
            }
        }

        Ok(())
    }

    fn start_service(&mut self, index: usize) -> SmResult {
        let service = &mut self.table.services[index];

        info!(name = %service.name, "Starting service");

        *service.status.lock()? = ServiceStatus::Running;

        let mut cmd = Command::new("sh");

        cmd.args(["-c", &service.command]);
        cmd.envs(service.env.iter());

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(Stdio::piped());

        let stdout_send = service.stdout.send.clone();

        let child = match cmd.spawn() {
            Err(err) => {
                let mut buf = [0; STDIO_SEND_BUF_SIZE];

                let bytes = err.to_string();

                (&mut buf[..]).write_all(bytes.as_bytes())?;

                stdout_send
                    .send((buf, bytes.len()))
                    .map_err(|_| SmError::FailedToSendStdio)?;

                return Err(SmError::FailedToStartChild(err));
            }
            Ok(child) => child,
        };

        let (terminate_send, terminate_recv) = mpsc::channel();

        self.thread_terminates.insert(index, terminate_send);

        let service_status = service.status.clone();
        let service_name = service.name.clone();

        let spawn_result = thread::Builder::new()
            .name(format!("worker-({})", service.name))
            .spawn(move || {
                match child_process_thread(
                    child,
                    stdout_send,
                    service_status,
                    service_name,
                    terminate_recv,
                ) {
                    Ok(_) => {}
                    Err(err) => {
                        error!(error = %err, "Error processing service");
                    }
                }
            });

        if let Err(err) = spawn_result {
            error!(error = %err, "Error spawning thread");
        }

        Ok(())
    }
}
