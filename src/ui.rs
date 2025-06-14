use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::app::{App, InputMode};

pub fn draw_loading(frame: &mut Frame, message: &str) {
    let area = frame.size();
    let block = Block::default().title(message).borders(Borders::ALL);
    frame.render_widget(block, area);
}

pub fn draw_directory(frame: &mut Frame, app: &App, state: &mut ListState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(frame.size());

    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .map(|&i| ListItem::new(app.chapters[i].title.clone()))
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Chapters"))
        .highlight_symbol(">>");
    frame.render_stateful_widget(list, chunks[0], state);

    let search = Paragraph::new(app.search.as_str()).block(
        Block::default().borders(Borders::ALL).title(match app.mode {
            InputMode::Navigate => "Press '/' to search",
            InputMode::Search => "Search",
        }),
    );
    frame.render_widget(search, chunks[1]);
}

pub fn draw_reading(frame: &mut Frame, app: &App) {
    let area = frame.size();
    let para = Paragraph::new(app.translation.as_str())
        .block(Block::default().borders(Borders::ALL).title("Translation"));
    frame.render_widget(para, area);
}
