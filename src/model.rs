use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{mpsc, Mutex};
use tui::widgets::TableState;

#[derive(Debug)]
pub struct App {
    pub table: AppState,
    pub selected: Option<usize>,
    pub thread_terminates: Vec<mpsc::Sender<()>>,
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
    pub status: Mutex<ServiceStatus>,
    pub stdout_buf: OsString,
    pub stdout_recv: mpsc::Receiver<Vec<u8>>,
    pub stdout_send: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum ServiceStatus {
    NotStarted,
    Running,
    Exited,
    Failed(u8),
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
