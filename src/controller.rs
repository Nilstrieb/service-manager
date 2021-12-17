use crate::model::config::Config;
use crate::model::{AppState, Service, ServiceStatus};
use crate::{view, App};
use crossterm::event;
use crossterm::event::{Event, KeyCode};
use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::io::{ErrorKind, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::TryRecvError;
use std::sync::{mpsc, Mutex};
use tui::backend::Backend;
use tui::widgets::TableState;
use tui::Terminal;

const STDOUT_SEND_BUF_SIZE: usize = 512;

pub type StdoutSendBuf = ([u8; STDOUT_SEND_BUF_SIZE], usize);

pub fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> io::Result<()> {
    loop {
        terminal.draw(|f| view::render_ui(f, &mut app))?;

        app.recv_stdouts();

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => match app.selected {
                    Some(_) => app.leave_service(),
                    None => break,
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

    // terminate the child processes
    for sender in app.thread_terminates {
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
                            status: Mutex::new(ServiceStatus::NotStarted),
                            stdout_buf: Vec::new(),
                            stdout_recv,
                            stdout_send: Mutex::new(Some(stdout_send)),
                        })
                    })
                    .collect::<io::Result<_>>()?,
            },
            selected: None,
            thread_terminates: Vec::new(),
        })
    }

    pub fn is_table(&self) -> bool {
        self.selected.is_none()
    }

    fn recv_stdouts(&mut self) {
        for service in self.table.services.iter_mut() {
            while let Ok((buf, n)) = service.stdout_recv.try_recv() {
                service.stdout_buf.extend_from_slice(&buf[0..n]);

                std::fs::write(
                    format!(
                        "debug/received_something_{}_{}.txt",
                        &service.name,
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis()
                    ),
                    &service.stdout_buf,
                )
                .expect("debug failed fuck");
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

    fn run_service(&mut self) {
        if let Some(selected) = self.selected {
            self.start_service(selected)
        } else if let Some(selected) = self.table.table_state.selected() {
            self.start_service(selected)
        }
    }

    fn start_service(&mut self, service: usize) {
        let service = &mut self.table.services[service];

        *service.status.lock().expect("service.status lock poisoned") = ServiceStatus::Running;

        let stdout_send = service
            .stdout_send
            .lock()
            .expect("stdout_send lock poisoned")
            .take()
            .expect("stdout_send has been stolen");

        let mut cmd = Command::new("sh");

        cmd.args(["-c", &service.command]);
        cmd.envs(service.env.iter());

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(Stdio::piped());

        let child = match cmd.spawn() {
            Err(err) => {
                let mut buf = [0; STDOUT_SEND_BUF_SIZE];

                let bytes = err.to_string();
                (&mut buf[..]).write_all(bytes.as_bytes()).expect("dont");

                stdout_send
                    .send((buf, bytes.len()))
                    .expect("failed to send stdout");
                return;
            }
            Ok(child) => child,
        };

        let (tx, rx) = mpsc::channel();

        self.thread_terminates.push(tx);

        std::thread::spawn(move || match child_process_thread(child, stdout_send, rx) {
            Ok(_) => {}
            Err(e) => std::fs::write("error.txt", e.to_string()).unwrap(),
        });
    }
}
fn child_process_thread(
    child: Child,
    stdout_send: mpsc::Sender<StdoutSendBuf>,
    terminate_channel: mpsc::Receiver<()>,
) -> io::Result<()> {
    let mut child = child;
    let mut stdout = child.stdout.take().unwrap();

    loop {
        match terminate_channel.try_recv() {
            Ok(_) | Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }

        let mut stdout_buf = [0; STDOUT_SEND_BUF_SIZE];

        match stdout.read(&mut stdout_buf) {
            Ok(0) => continue,
            Ok(n) => {
                // std::fs::write(
                //     format!(
                //         "debug/read_something_{}.txt",
                //         std::time::SystemTime::now()
                //             .duration_since(std::time::UNIX_EPOCH)
                //             .unwrap()
                //             .as_millis()
                //     ),
                //     &stdout_buf,
                // )
                // .ok();

                stdout_send
                    .send((stdout_buf, n))
                    .map_err(|_| io::Error::from(io::ErrorKind::Other))?;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        };
    }

    match child.kill() {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::InvalidInput => {}
        Err(e) => return Err(e),
    }

    Ok(())
}
