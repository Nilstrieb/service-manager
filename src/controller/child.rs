use crate::controller::{StdioSendBuf, STDIO_SEND_BUF_SIZE};
use crate::model::{ServiceStatus, SmError, SmResult};
use std::io::{Read, Write};
use std::process::{Child, ChildStderr, ChildStdout};
use std::sync::mpsc::TryRecvError;
use std::sync::{mpsc, Arc, Mutex};
use std::{io, thread};
use tracing::{error, info};

enum ChildAction {
    Ignore,
    Kill,
}

pub fn child_process_thread(
    mut child: Child,
    mut stdout_send: mpsc::Sender<StdioSendBuf>,
    service_status: Arc<Mutex<ServiceStatus>>,
    service_name: String,
    terminate_channel: mpsc::Receiver<()>,
) -> SmResult {
    let stdout = child
        .stdout
        .take()
        .ok_or(SmError::Bug("Stdout of child could not be taken"))?;

    let stderr = child
        .stderr
        .take()
        .ok_or(SmError::Bug("Stderr of child could not be taken"))?;

    let (stderr_terminate_send, stderr_terminate_recv) = mpsc::channel();

    let stdout_send_2 = stdout_send.clone();

    let stderr_thread_result = thread::Builder::new()
        .name(format!("worker-stderr-({})", service_name))
        .spawn(move || child_process_stderr_thread(stdout_send_2, stderr_terminate_recv, stderr));

    if let Err(err) = stderr_thread_result {
        error!(error = %err, "Failed to spawn stderr thread");
    }

    info!(name = %service_name, "Entering main processing loop");
    let (action, result) = child_process_main_thread(
        &mut stdout_send,
        terminate_channel,
        stderr_terminate_send,
        stdout,
        &mut child,
        service_status.clone(),
    );

    if let ChildAction::Kill = action {
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
    }

    result
}

fn child_process_main_thread(
    stdout_send: &mut mpsc::Sender<StdioSendBuf>,
    terminate_channel: mpsc::Receiver<()>,
    stderr_terminate_send: mpsc::Sender<()>,
    mut stdout: ChildStdout,
    child: &mut Child,
    service_status: Arc<Mutex<ServiceStatus>>,
) -> (ChildAction, SmResult) {
    loop {
        match terminate_channel.try_recv() {
            Ok(_) | Err(TryRecvError::Disconnected) => {
                // terminating the thread is a best-effort, it doesn't matter if it died
                let _ = stderr_terminate_send.send(());
                return (ChildAction::Kill, Ok(()));
            }
            Err(TryRecvError::Empty) => {}
        }

        let mut stdout_buf = [0; STDIO_SEND_BUF_SIZE];
        match stdout.read(&mut stdout_buf) {
            Ok(0) => {}
            Ok(n) => match stdout_send.send((stdout_buf, n)) {
                Ok(()) => {}
                Err(_) => {
                    return (
                        ChildAction::Kill,
                        Err(SmError::Bug("Failed to send stdout to main thread")),
                    )
                }
            },
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            // todo: maybe we are overreacting here?
            Err(err) => return (ChildAction::Kill, Err(err.into())),
        };

        match child.try_wait() {
            Ok(None) => {}
            Ok(Some(status)) => {
                let mut status_lock = match service_status.lock() {
                    Ok(lock) => lock,
                    Err(_) => return (ChildAction::Kill, Err(SmError::MutexPoisoned)),
                };

                *status_lock = match status.code() {
                    Some(0) => ServiceStatus::Exited,
                    Some(code) => ServiceStatus::Failed(code),
                    None => ServiceStatus::Killed,
                };

                return (ChildAction::Ignore, Ok(()));
            }
            Err(err) => return (ChildAction::Kill, Err(err.into())),
        }
    }
}

fn child_process_stderr_thread(
    stdout_send: mpsc::Sender<StdioSendBuf>,
    terminate_channel: mpsc::Receiver<()>,
    mut stderr: ChildStderr,
) {
    info!("Entering main processing loop for stderr thread");

    loop {
        match terminate_channel.try_recv() {
            Ok(_) | Err(TryRecvError::Disconnected) => return,
            Err(TryRecvError::Empty) => {}
        }

        let mut stderr_buf = [0; STDIO_SEND_BUF_SIZE];
        match stderr.read(&mut stderr_buf) {
            Ok(0) => {}
            Ok(n) => {
                let result = stdout_send
                    .send((stderr_buf, n))
                    .map_err(|_| SmError::Bug("Failed to send stderr to main thread"));

                if let Err(err) = result {
                    error!(error = %err);
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => {
                error!(error = %err, "Error reading from stderr");
                return;
            }
        };
    }
}
