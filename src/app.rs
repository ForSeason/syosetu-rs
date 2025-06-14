use std::collections::HashMap;
use std::io::{self};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, MouseEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::prelude::*;
use ratatui::backend::CrosstermBackend;
use ratatui::widgets::ListState;

use crate::memory::KeywordStore;
use crate::syosetu::{Chapter, SyosetuClient};
use crate::ui::{draw_directory, draw_loading, draw_reading};

#[derive(Clone, Copy, PartialEq)]
pub enum InputMode {
    Navigate,
    Search,
}

#[derive(Clone, Copy, PartialEq)]
pub enum AppState {
    LoadingDir,
    Directory,
    LoadingChapter,
    Reading,
}

pub struct App {
    pub state: AppState,
    pub mode: InputMode,
    pub chapters: Vec<Chapter>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub search: String,
    pub content: String,
    pub translation: String,
    pub novel_id: String,
    pub keywords: HashMap<String, String>,
}

impl App {
    pub fn new(novel_id: String) -> Self {
        App {
            state: AppState::LoadingDir,
            mode: InputMode::Navigate,
            chapters: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            search: String::new(),
            content: String::new(),
            translation: String::new(),
            novel_id,
            keywords: HashMap::new(),
        }
    }

    pub fn apply_filter(&mut self) {
        if self.search.is_empty() {
            self.filtered = (0..self.chapters.len()).collect();
        } else {
            let q = self.search.to_lowercase();
            self.filtered = self
                .chapters
                .iter()
                .enumerate()
                .filter_map(|(i, ch)| {
                    if ch.title.to_lowercase().contains(&q) || (i + 1).to_string().contains(&q) {
                        Some(i)
                    } else {
                        None
                    }
                })
                .collect();
        }
        if self.selected >= self.filtered.len() {
            self.selected = 0;
        }
    }

    pub async fn run(mut self, url: &str, client: &SyosetuClient, store: &dyn KeywordStore) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        terminal.draw(|f| draw_loading(f, "Loading directory..."))?;
        let chapters = client.fetch_directory(url).await?;
        self.chapters = chapters;
        self.apply_filter();
        self.state = AppState::Directory;

        self.keywords = store.load(&self.novel_id)?;

        let mut list_state = ListState::default();
        list_state.select(Some(0));

        let tick_rate = Duration::from_millis(200);
        let mut last_tick = Instant::now();
        loop {
            terminal.draw(|f| match self.state {
                AppState::LoadingDir => draw_loading(f, "Loading directory..."),
                AppState::Directory => draw_directory(f, &self, &mut list_state),
                AppState::LoadingChapter => draw_loading(f, "Loading chapter..."),
                AppState::Reading => draw_reading(f, &self),
            })?;

            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(k) => match self.state {
                        AppState::Directory => match self.mode {
                            InputMode::Navigate => match k.code {
                                KeyCode::Char('j') | KeyCode::Down => {
                                    if self.selected + 1 < self.filtered.len() {
                                        self.selected += 1;
                                        list_state.select(Some(self.selected));
                                    }
                                }
                                KeyCode::Char('k') | KeyCode::Up => {
                                    if self.selected > 0 {
                                        self.selected -= 1;
                                        list_state.select(Some(self.selected));
                                    }
                                }
                                KeyCode::Enter => {
                                    if let Some(&idx) = self.filtered.get(self.selected) {
                                        let chapter = &self.chapters[idx];
                                        self.state = AppState::LoadingChapter;
                                        terminal.draw(|f| draw_loading(f, "Loading chapter..."))?;
                                        let content = client.fetch_chapter(&chapter.path).await?;
                                        self.content = content.clone();
                                        let existing: Vec<(String, String)> = self
                                            .keywords
                                            .iter()
                                            .map(|(k, v)| (k.clone(), v.clone()))
                                            .collect();
                                        let trans = client.translate_text(&content, &existing).await?;
                                        self.translation = trans.clone();
                                        let existing_lines: Vec<String> = existing
                                            .iter()
                                            .map(|(jp, zh)| {
                                                format!("{{\"japanese\":\"{}\",\"chinese\":\"{}\"}}", jp, zh)
                                            })
                                            .collect();
                                        let new_keywords = client
                                            .extract_keywords(&self.translation, &self.content, existing_lines)
                                            .await?;
                                        for line in new_keywords {
                                            if let Ok(val) = serde_json::from_str::<HashMap<String, String>>(&line) {
                                                if let (Some(jp), Some(zh)) = (val.get("japanese"), val.get("chinese")) {
                                                    self.keywords.entry(jp.to_string()).or_insert(zh.to_string());
                                                }
                                            }
                                        }
                                        store.save(&self.novel_id, &self.keywords)?;
                                        self.state = AppState::Reading;
                                    }
                                }
                                KeyCode::Char('/') => {
                                    self.mode = InputMode::Search;
                                    self.search.clear();
                                }
                                KeyCode::Char('q') => break,
                                _ => {}
                            },
                            InputMode::Search => match k.code {
                                KeyCode::Esc => {
                                    self.mode = InputMode::Navigate;
                                }
                                KeyCode::Enter => {
                                    self.apply_filter();
                                    list_state.select(Some(self.selected));
                                    self.mode = InputMode::Navigate;
                                }
                                KeyCode::Backspace => {
                                    self.search.pop();
                                }
                                KeyCode::Char(c) => {
                                    self.search.push(c);
                                }
                                _ => {}
                            },
                        },
                        AppState::Reading => match k.code {
                            KeyCode::Char('q') => break,
                            KeyCode::Char('b') => {
                                self.state = AppState::Directory;
                            }
                            _ => {}
                        },
                        _ => {}
                    },
                    Event::Mouse(m) => {
                        if self.state == AppState::Directory {
                            if let MouseEventKind::Down(_) = m.kind {
                                let row = m.row as usize;
                                if row < self.filtered.len() {
                                    self.selected = row;
                                    list_state.select(Some(self.selected));
                                }
                            }
                        }
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }

            if last_tick.elapsed() >= tick_rate {
                last_tick = Instant::now();
            }
        }

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;
        Ok(())
    }
}
