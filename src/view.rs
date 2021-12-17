use tui::backend::Backend;
use tui::layout::{Constraint, Layout};
use tui::style::{Color, Modifier, Style};
use tui::widgets::{Block, Borders, Cell, Row, Table};
use tui::Frame;

use crate::model::{AppState, AppStateTable};
use crate::App;

pub fn ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    match &mut app.state {
        AppState::Table(state) => {
            table_ui(f, state);
        }
        AppState::FullView { name } => f.render_widget(
            Block::default().borders(Borders::ALL).title(name.as_ref()),
            f.size(),
        ),
    }
}

pub fn table_ui<B: Backend>(f: &mut Frame<B>, state: &mut AppStateTable) {
    let rects = Layout::default()
        .constraints(vec![Constraint::Percentage(100)])
        .margin(5)
        .split(f.size());

    let selected_style = Style::default().add_modifier(Modifier::REVERSED);
    let normal_style = Style::default().bg(Color::Blue);
    let header_cells = ["Header1", "Header2", "Header3"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Red)));

    let header = Row::new(header_cells)
        .style(normal_style)
        .height(1)
        .bottom_margin(1);

    let rows = state.items.iter().map(|item| {
        let height = item
            .iter()
            .map(|content| content.chars().filter(|c| *c == '\n').count())
            .max()
            .unwrap_or(0)
            + 1;
        let cells = item.iter().map(|c| Cell::from(c.as_ref()));
        Row::new(cells).height(height as u16).bottom_margin(1)
    });

    let t = Table::new(rows)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Table"))
        .highlight_style(selected_style)
        .highlight_symbol(">> ")
        .widths(&[
            Constraint::Percentage(50),
            Constraint::Length(30),
            Constraint::Min(10),
        ]);

    f.render_stateful_widget(t, rects[0], &mut state.table_state);
}
