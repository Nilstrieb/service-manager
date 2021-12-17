use tui::widgets::TableState;

#[derive(Debug)]
pub struct App {
    pub table: AppState,
    pub selected: Option<AppStateFullView>,
}

#[derive(Debug)]
pub struct AppState {
    pub table_state: TableState,
    pub items: Vec<Service>,
}

#[derive(Debug)]
pub struct AppStateFullView {
    pub index: usize,
}

#[derive(Debug)]
pub struct Service {
    pub name: String,
    pub status: ServiceStatus,
}

#[derive(Debug)]
pub enum ServiceStatus {
    Running,
    Exited,
    Failed(u8),
}
