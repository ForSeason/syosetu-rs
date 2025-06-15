use std::collections::{HashMap, HashSet};
use std::io::{self};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, MouseEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::prelude::*;
use ratatui::backend::CrosstermBackend;
use ratatui::widgets::ListState;
use std::sync::{Arc, Mutex};

use crate::memory::{KeywordStore, TranslationStore};
use crate::syosetu::{Chapter, NovelSite, Translator};
use crate::ui::{draw_directory, draw_loading, draw_reading};
use tokio::task::JoinHandle;

/// 应用在目录界面中的输入模式
#[derive(Clone, Copy, PartialEq)]
pub enum InputMode {
    /// 普通浏览模式，可上下移动光标
    Navigate,
    /// 输入搜索关键词
    Search,
}

/// 程序当前所处的状态
#[derive(Clone, Copy, PartialEq)]
pub enum AppState {
    /// 正在加载目录
    LoadingDir,
    /// 显示目录列表
    Directory,
    /// 阅读模式
    Reading,
}

/// 保存 UI 状态及缓存数据
pub struct App {
    /// 当前所处的状态
    pub state: AppState,
    /// 目录界面的输入模式
    pub mode: InputMode,
    /// 全部章节列表
    pub chapters: Vec<Chapter>,
    /// 根据搜索过滤后的索引
    pub filtered: Vec<usize>,
    /// 当前选中项在 `filtered` 中的索引
    pub selected: usize,
    /// 搜索框内容
    pub search: String,
    /// 翻译结果
    pub translation: String,
    /// 阅读时的滚动位置
    pub scroll: u16,
    /// 小说的唯一 id
    pub novel_id: String,
    /// 已知的翻译对照表
    pub keywords: Arc<Mutex<HashMap<String, String>>>,
    /// 本地已缓存章节路径
    pub cached_chapters: HashSet<String>,
    /// 正在处理的章节任务
    pub processing: HashMap<String, JoinHandle<anyhow::Result<String>>>,
    /// 当前阅读的章节路径
    pub current_chapter: Option<String>,
}

impl App {
    /// 根据小说 id 创建新的应用状态
    pub fn new(novel_id: String) -> Self {
        App {
            state: AppState::LoadingDir,
            mode: InputMode::Navigate,
            chapters: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            search: String::new(),
            translation: String::new(),
            scroll: 0,
            novel_id,
            keywords: Arc::new(Mutex::new(HashMap::new())),
            cached_chapters: HashSet::new(),
            processing: HashMap::new(),
            current_chapter: None,
        }
    }

    /// 根据搜索框内容重新过滤章节列表
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

    fn spawn_processing(
        &mut self,
        chapter: Chapter,
        site: Arc<dyn NovelSite>,
        translator: Arc<Translator>,
        kw_store: Arc<dyn KeywordStore>,
        trans_store: Arc<dyn TranslationStore>,
    ) {
        if self.processing.contains_key(&chapter.path) {
            return;
        }
        let path = chapter.path.clone();
        let novel_id = self.novel_id.clone();
        let keywords = self.keywords.clone();
        let handle: JoinHandle<anyhow::Result<String>> = tokio::spawn(async move {
            let content = site.fetch_chapter(&path).await?;
            let existing: Vec<(String, String)> = {
                let kw = keywords.lock().unwrap();
                kw.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            };
            let trans = translator.translate_text(&content, &existing).await?;
            let existing_lines: Vec<String> = existing
                .iter()
                .map(|(jp, zh)| format!("{{\"japanese\":\"{}\",\"chinese\":\"{}\"}}", jp, zh))
                .collect();
            let new_keywords = translator.extract_keywords(&trans, &content, existing_lines).await?;
            {
                let mut kw_lock = keywords.lock().unwrap();
                for line in new_keywords {
                    if let Ok(val) = serde_json::from_str::<HashMap<String, String>>(&line) {
                        if let (Some(jp), Some(zh)) = (val.get("japanese"), val.get("chinese")) {
                            kw_lock.entry(jp.to_string()).or_insert(zh.to_string());
                        }
                    }
                }
                kw_store.save(&novel_id, &kw_lock)?;
            }
            trans_store.save(&novel_id, &path, &trans)?;
            Ok(trans)
        });
        self.processing.insert(chapter.path.clone(), handle);
    }

    /// 主事件循环，处理渲染与用户输入
    pub async fn run(
        mut self,
        url: &str,
        site: Arc<dyn NovelSite>,
        translator: Arc<Translator>,
        kw_store: Arc<dyn KeywordStore>,
        trans_store: Arc<dyn TranslationStore>,
    ) -> Result<()> {
        // 初始化终端并进入全屏模式
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // 读取目录
        terminal.draw(|f| draw_loading(f, "Loading directory..."))?;
        let chapters = site.fetch_directory(url).await?;
        self.chapters = chapters;
        self.apply_filter();
        self.state = AppState::Directory;

        // 加载翻译对照表以及已缓存章节列表
        {
            let mut kw_lock = self.keywords.lock().unwrap();
            *kw_lock = kw_store.load(&self.novel_id)?;
        }
        self.cached_chapters = trans_store
            .list(&self.novel_id)?
            .into_iter()
            .collect();

        // `ListState` 用于追踪列表光标位置
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        // 主循环：定期刷新界面并处理用户输入
        let tick_rate = Duration::from_millis(200);
        let mut last_tick = Instant::now();
        loop {
            terminal.draw(|f| match self.state {
                AppState::LoadingDir => draw_loading(f, "Loading directory..."),
                AppState::Directory => draw_directory(f, &self, &mut list_state),
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
                                        self.scroll = 0;
                                        if let Some(trans) =
                                            trans_store.load(&self.novel_id, &chapter.path)?
                                        {
                                            self.current_chapter = Some(chapter.path.clone());
                                            self.translation = trans;
                                            self.state = AppState::Reading;
                                        } else if self.processing.contains_key(&chapter.path) {
                                            self.current_chapter = Some(chapter.path.clone());
                                            self.translation = "Processing...".to_string();
                                            self.state = AppState::Reading;
                                        } else {
                                            self.spawn_processing(
                                                chapter.clone(),
                                                site.clone(),
                                                translator.clone(),
                                                kw_store.clone(),
                                                trans_store.clone(),
                                            );
                                            // stay in directory so user can queue more tasks
                                        }
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
                            KeyCode::Char('q') | KeyCode::Esc => {
                                self.state = AppState::Directory;
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                self.scroll = self.scroll.saturating_add(1);
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                self.scroll = self.scroll.saturating_sub(1);
                            }
                            KeyCode::PageDown => {
                                let h = terminal.size()?.height;
                                self.scroll = self
                                    .scroll
                                    .saturating_add(h.saturating_sub(1));
                            }
                            KeyCode::PageUp => {
                                let h = terminal.size()?.height;
                                self.scroll = self
                                    .scroll
                                    .saturating_sub(h.saturating_sub(1));
                            }
                            _ => {}
                        },
                        _ => {}
                    },
                    Event::Mouse(m) => {
                        match self.state {
                            AppState::Directory => {
                                if let MouseEventKind::Down(_) = m.kind {
                                    let row = m.row as usize;
                                    if row < self.filtered.len() {
                                        self.selected = row;
                                        list_state.select(Some(self.selected));
                                    }
                                }
                            }
                            AppState::Reading => match m.kind {
                                MouseEventKind::ScrollDown => {
                                    self.scroll = self.scroll.saturating_add(1);
                                }
                                MouseEventKind::ScrollUp => {
                                    self.scroll = self.scroll.saturating_sub(1);
                                }
                                _ => {}
                            },
                            _ => {}
                        }
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }

            // 检查后台任务是否完成
            let finished: Vec<String> = self
                .processing
                .iter()
                .filter_map(|(p, h)| if h.is_finished() { Some(p.clone()) } else { None })
                .collect();
            for path in finished {
                if let Some(handle) = self.processing.remove(&path) {
                    match handle.await {
                        Ok(Ok(trans)) => {
                            self.cached_chapters.insert(path.clone());
                            if self.current_chapter.as_deref() == Some(&path) {
                                self.translation = trans;
                            }
                        }
                        Ok(Err(e)) => {
                            if self.current_chapter.as_deref() == Some(&path) {
                                self.translation = format!("Error: {e}");
                            }
                        }
                        Err(e) => {
                            if self.current_chapter.as_deref() == Some(&path) {
                                self.translation = format!("Task error: {e}");
                            }
                        }
                    }
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
