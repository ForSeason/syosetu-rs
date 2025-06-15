use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::app::{App, InputMode};

/// 在全屏区域绘制一个带标题的空白块，用于提示加载状态
pub fn draw_loading(frame: &mut Frame, message: &str) {
    let area = frame.size();
    let block = Block::default().title(message).borders(Borders::ALL);
    frame.render_widget(block, area);
}

/// 章节目录界面的渲染函数
pub fn draw_directory(frame: &mut Frame, app: &App, state: &mut ListState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(frame.size());

    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .map(|&i| {
            let ch = &app.chapters[i];
            let mark = if app.cached_chapters.contains(&ch.path) {
                "[C] "
            } else if app.processing_chapters.contains(&ch.path) {
                "[P] "
            } else {
                "[ ] "
            };
            ListItem::new(format!("{}{}", mark, ch.title))
        })
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

/// 显示翻译文本并根据滚动位置偏移
pub fn draw_reading(frame: &mut Frame, app: &App) {
    let area = frame.size();
    let para = Paragraph::new(app.translation.as_str())
        .block(Block::default().borders(Borders::ALL).title("Translation"))
        .scroll((app.scroll, 0));
    frame.render_widget(para, area);
}
