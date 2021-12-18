use crate::controller::StdioSendBuf;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use tui::widgets::TableState;

pub use error::{SmError, SmResult};

#[derive(Debug)]
pub struct App {
    pub table: AppState,
    pub selected: Option<usize>,
    pub thread_terminates: HashMap<usize, mpsc::Sender<()>>,
}

#[derive(Debug)]
pub struct AppState {
    pub table_state: TableState,
    pub services: Vec<Service>,
}

#[derive(Debug)]
pub struct Service {
    pub command: String,
    pub name: String,
    pub workdir: PathBuf,
    pub env: HashMap<String, String>,
    pub status: Arc<Mutex<ServiceStatus>>,
    pub std_io_buf: Vec<u8>,
    pub stdout: StdIoStream,
}

#[derive(Debug)]
pub struct StdIoStream {
    pub recv: mpsc::Receiver<StdioSendBuf>,
    pub send: mpsc::Sender<StdioSendBuf>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ServiceStatus {
    NotStarted,
    Running,
    Exited,
    Failed(u8),
    Killed,
}

pub mod config {
    use serde::Deserialize;
    use std::collections::{BTreeMap, HashMap};
    use std::path::PathBuf;

    pub type Config = BTreeMap<String, Service>;

    #[derive(Debug, Deserialize)]
    pub struct Service {
        pub command: String,
        pub workdir: Option<PathBuf>,
        pub env: Option<HashMap<String, String>>,
    }
}
mod error {
    use std::fmt::{Display, Formatter};
    use std::io;
    use std::sync::PoisonError;

    pub type SmResult = Result<(), SmError>;

    pub enum SmError {
        Io(io::Error),
        FailedToStartChild(io::Error),
        MutexPoisoned,
        FailedToSendStdio,
        /// This should never happen and would be a panic in most programs, but panicking here
        /// might fuck things up badly, so we don't want to
        Bug(&'static str),
    }

    impl From<io::Error> for SmError {
        fn from(e: io::Error) -> Self {
            Self::Io(e)
        }
    }

    impl<T> From<std::sync::PoisonError<T>> for SmError {
        fn from(_: PoisonError<T>) -> Self {
            Self::MutexPoisoned
        }
    }

    impl Display for SmError {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Io(e) => Display::fmt(e, f),
                SmError::MutexPoisoned => f.write_str("Mutex was poisoned. This is a bug."),
                SmError::FailedToStartChild(e) => write!(f, "Failed to start child process: {}", e),
                SmError::FailedToSendStdio => {
                    f.write_str("Failed to send stdio to display thread. This is a bug.")
                }
                SmError::Bug(str) => write!(f, "{}. This is a bug.", str),
            }
        }
    }
}
