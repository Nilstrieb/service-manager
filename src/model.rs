use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::process::Child;
use tui::widgets::TableState;

#[derive(Debug)]
pub struct App {
    pub table: AppState,
    pub selected: Option<usize>,
}

#[derive(Debug)]
pub struct AppState {
    pub table_state: TableState,
    pub items: Vec<Service>,
}

#[derive(Debug)]
pub struct Service {
    pub command: String,
    pub name: String,
    pub workdir: PathBuf,
    pub env: HashMap<String, String>,
    pub status: ServiceStatus,
    pub child: Option<io::Result<Child>>,
}

#[derive(Debug)]
pub enum ServiceStatus {
    NotStarted,
    Running,
    Exited,
    Failed(u8),
}

pub mod config {
    use serde::Deserialize;
    use std::collections::{BTreeMap, HashMap};
    use std::ffi::OsString;
    use std::path::PathBuf;

    pub type Config = BTreeMap<String, Service>;

    #[derive(Debug, Deserialize)]
    pub struct Service {
        pub command: String,
        pub workdir: Option<PathBuf>,
        pub env: Option<HashMap<String, String>>,
    }
}
