use tui::widgets::TableState;

#[derive(Debug)]
pub struct App {
    pub state: AppState
}

#[derive(Debug)]
pub enum AppState {
    Table(AppStateTable),
    FullView {
        name: String,
    }
}

#[derive(Debug)]
pub struct AppStateTable {
    pub table_state: TableState,
    pub items: Vec<Vec<String>>,
}

#[derive(Debug)]
pub struct AppStateFullView {
    name: String,
}