use crate::model::config::Config;
use crate::model::{AppState, Service, ServiceStatus, SmError, SmResult, StdIoStream};
use crate::{view, App};
use crossterm::event;
use crossterm::event::{Event, KeyCode};
use std::collections::HashMap;
use std::io;
use std::io::{ErrorKind, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::TryRecvError;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tracing::{error, info, trace_span};
use tui::backend::Backend;
use tui::widgets::TableState;
use tui::Terminal;

const STDIO_SEND_BUF_SIZE: usize = 512;

pub type StdioSendBuf = ([u8; STDIO_SEND_BUF_SIZE], usize);

pub fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> SmResult {
    info!("Entering main loop");

    loop {
        terminal.draw(|f| view::render_ui(f, &mut app))?;

        app.recv_stdouts();

        if event::poll(Duration::from_millis(10))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => match app.selected {
                        Some(_) => app.leave_service(),
                        None => break,
                    },
                    KeyCode::Char('r') => app.run_service()?,
                    KeyCode::Char('k') => app.kill_service()?,
                    KeyCode::Down => app.next(),
                    KeyCode::Up => app.previous(),
                    KeyCode::Enter => app.select_service(),
                    KeyCode::Esc => app.leave_service(),
                    _ => {}
                }
            }
        }
    }

    // terminate the child processes
    for sender in app.thread_terminates.values() {
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
                            stdout: StdIoStream {
                                recv: stdout_recv,
                                send: stdout_send,
                            },
                        })
                    })
                    .collect::<io::Result<_>>()?,
            },
            selected: None,
            thread_terminates: HashMap::new(),
        })
    }

    pub fn is_table(&self) -> bool {
        self.selected.is_none()
    }

    fn recv_stdouts(&mut self) {
        for service in self.table.services.iter_mut() {
            while let Ok((buf, n)) = service.stdout.recv.try_recv() {
                service.std_io_buf.extend(&buf[0..n]);

                if service.std_io_buf.len() > 2_000_000 {
                    service.std_io_buf.clear(); // todo don't
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

    fn run_service(&mut self) -> SmResult {
        let index = self.selected.or_else(|| self.table.table_state.selected());

        if let Some(index) = index {
            let status = {
                let service = &mut self.table.services[index];
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
            let status = {
                let service = &mut self.table.services[index];
                *service.status.lock()?
            };

            if status == ServiceStatus::Running {
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

        trace_span!("Starting service", name = %service.name);

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

        let spawn_result = std::thread::Builder::new()
            .name(format!("worker-{}", service.name))
            .spawn(move || {
                match child_process_thread(child, stdout_send, service_status, terminate_recv) {
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

fn child_process_thread(
    child: Child,
    stdout_send: mpsc::Sender<StdioSendBuf>,
    service_status: Arc<Mutex<ServiceStatus>>,
    terminate_channel: mpsc::Receiver<()>,
) -> SmResult {
    let mut child = child;
    let mut stdout = child
        .stdout
        .take()
        .ok_or(SmError::Bug("Stdout of child could not be taken"))?;

    let mut stderr = child
        .stderr
        .take()
        .ok_or(SmError::Bug("Stderr of child could not be taken"))?;

    let stdout_send_2 = stdout_send.clone();
    std::thread::spawn(move || {
        let mut stderr_buf = [0; STDIO_SEND_BUF_SIZE];
        match stderr.read(&mut stderr_buf) {
            Ok(0) => {}
            Ok(n) => {
                let result = stdout_send_2
                    .send((stderr_buf, n))
                    .map_err(|_| SmError::Bug("Failed to send stderr to main thread"));

                if let Err(err) = result {
                    error!(error = %err);
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => error!(error = %err, "Error reading from stderr"),
        };
    });

    let result = loop {
        match terminate_channel.try_recv() {
            Ok(_) | Err(TryRecvError::Disconnected) => break Ok(()),
            Err(TryRecvError::Empty) => {}
        }

        let mut stdout_buf = [0; STDIO_SEND_BUF_SIZE];
        match stdout.read(&mut stdout_buf) {
            Ok(0) => {}
            Ok(n) => {
                stdout_send
                    .send((stdout_buf, n))
                    .map_err(|_| SmError::Bug("Failed to send stdout to main thread"))?;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => break Err(e.into()),
        };

        match child.try_wait() {
            Ok(None) => {}
            Ok(Some(status)) => {
                let mut status_lock = service_status.lock().map_err(|_| SmError::MutexPoisoned)?;

                *status_lock = match status.code() {
                    Some(0) => ServiceStatus::Exited,
                    Some(code) => ServiceStatus::Failed(code),
                    None => ServiceStatus::Killed,
                };

                return Ok(());
            }
            Err(e) => break Err(e.into()),
        }
    };

    match child.kill() {
        Ok(()) => {
            *service_status.lock().map_err(|_| SmError::MutexPoisoned)? = ServiceStatus::Killed
        }
        Err(e) if e.kind() == io::ErrorKind::InvalidInput => {}
        Err(e) => return Err(e.into()),
    }

    let mut send_message_buf = [0; STDIO_SEND_BUF_SIZE];
    let kill_msg = "\n\n<Process was killed>\n";
    send_message_buf
        .as_mut_slice()
        .write_all(kill_msg.as_bytes())?;
    stdout_send
        .send((send_message_buf, kill_msg.len()))
        .map_err(|_| SmError::Bug("Failed to send stdout to main thread"))?;

    result
}
