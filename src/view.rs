use std::fmt::{Display, Formatter};
use tui::backend::Backend;
use tui::layout::{Constraint, Layout, Rect};
use tui::style::{Color, Modifier, Style};
use tui::text::Spans;
use tui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use tui::Frame;

use crate::model::{AppState, ServiceStatus};
use crate::App;

pub fn render_ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let chunks = if f.size().height < 22 {
        Layout::default()
            .constraints(vec![Constraint::Percentage(100)])
            .split(f.size())
    } else {
        Layout::default()
            .constraints(vec![Constraint::Percentage(90), Constraint::Max(3)])
            .split(f.size())
    };

    match app.selected {
        None => {
            render_table(f, &mut app.table, chunks[0]);
        }
        Some(index) => {
            render_full_view(f, &mut app.table, index, chunks[0]);
        }
    }

    if let Some(footer_chunk) = chunks.get(1) {
        render_help_footer(f, app, *footer_chunk);
    }
}

fn render_full_view<B: Backend>(f: &mut Frame<B>, state: &mut AppState, index: usize, area: Rect) {
    let service = &state.services[index];

    let block = Block::default()
        .borders(Borders::ALL)
        .title("service".as_ref());

    let stdout = service.stdout_buf.to_string_lossy();

    let paragraph = Paragraph::new(stdout.as_ref()).block(block);

    f.render_widget(paragraph, area)
}

fn render_table<B: Backend>(f: &mut Frame<B>, state: &mut AppState, area: Rect) {
    let selected_style = Style::default().add_modifier(Modifier::REVERSED);
    let normal_style = Style::default().bg(Color::Blue);
    let header_cells = ["name", "status"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default()));

    let header = Row::new(header_cells)
        .style(normal_style)
        .height(1)
        .bottom_margin(1);

    let rows = state.services.iter().map(|service| {
        let height = service.name.chars().filter(|c| *c == '\n').count() + 1;

        let cells = [
            Cell::from(service.name.as_ref()),
            Cell::from(
                service
                    .status
                    .lock()
                    .expect("service.status lock poisoned")
                    .to_string(),
            ),
        ];
        Row::new(cells).height(height as u16).bottom_margin(1)
    });

    let t = Table::new(rows)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("services"))
        .highlight_style(selected_style)
        .widths(&[Constraint::Percentage(50), Constraint::Length(30)]);

    f.render_stateful_widget(t, area, &mut state.table_state);
}

fn render_help_footer<B: Backend>(f: &mut Frame<B>, app: &App, area: Rect) {
    let block = Block::default().title("help").borders(Borders::ALL);

    let paragraph = Paragraph::new(if app.is_table() {
        vec![Spans::from(
            "q-quit    down-down    up-up    enter-select    r-run service",
        )]
    } else {
        vec![Spans::from("q-back    esc-back    r-run service")]
    })
    .block(block);

    f.render_widget(paragraph, area);
}

impl Display for ServiceStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceStatus::Running => f.write_str("running"),
            ServiceStatus::Exited => f.write_str("exited (0)"),
            ServiceStatus::Failed(code) => write!(f, "failed ({})", code),
            ServiceStatus::NotStarted => f.write_str("not started"),
        }
    }
}
