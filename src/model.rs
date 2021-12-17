use tui::widgets::TableState;

#[derive(Debug)]
pub struct App {
    pub table: AppState,
    pub selected: Option<AppStateFullView>
}

#[derive(Debug)]
pub struct AppState {
    pub table_state: TableState,
    pub items: Vec<Vec<String>>,
}

#[derive(Debug)]
pub struct AppStateFullView {
    pub name: String,
}
