use crate::{
    document::{Document, Kind},
    layout::{DiagramState, Layout},
    source,
    terminal::Capabilities,
    watch::Watch,
    worker::{Completion, Job, Worker},
};
use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Files,
    Toc,
}
#[derive(Debug, Clone)]
pub struct Action {
    pub block: usize,
    pub target: Option<String>,
    pub label: String,
}
#[derive(Debug, Clone)]
pub struct Detail {
    pub block: usize,
    pub x: usize,
    pub y: usize,
    pub zoom: f64,
}
pub struct App {
    pub path: Option<PathBuf>,
    pub root: PathBuf,
    pub files: Vec<PathBuf>,
    pub source: String,
    pub doc: Document,
    pub diagrams: HashMap<usize, DiagramState>,
    pub layout: Layout,
    pub cap: Capabilities,
    pub scroll: usize,
    pub horizontal: usize,
    pub width: usize,
    pub height: usize,
    pub panel: Option<Panel>,
    pub panel_index: usize,
    pub panel_focus: bool,
    pub actions: Vec<Action>,
    pub selected: Option<usize>,
    pub help: bool,
    pub help_scroll: usize,
    pub detail: Option<Detail>,
    pub query: String,
    pub searching: bool,
    pub matches: Vec<usize>,
    pub message: String,
    pub dirty: bool,
    pub version: u64,
    pub watching: bool,
    pub watch: Option<Watch>,
    pub worker: Worker,
    pending: Option<Job>,
    history: Vec<(Option<PathBuf>, String, usize, usize)>,
    pub quit: bool,
}
impl App {
    pub fn new(
        path: Option<PathBuf>,
        root: PathBuf,
        source: String,
        directory: bool,
        watch: bool,
        cap: Capabilities,
        font: Option<PathBuf>,
    ) -> Self {
        let watcher = if watch {
            path.as_deref().and_then(|p| Watch::new(p).ok())
        } else {
            None
        };
        let mut app = Self {
            path,
            files: source::markdown_files(&root),
            root,
            source: String::new(),
            doc: Document::default(),
            diagrams: HashMap::new(),
            layout: Layout::default(),
            scroll: 0,
            horizontal: 0,
            width: 80,
            height: 24,
            panel: directory.then_some(Panel::Files),
            panel_index: 0,
            panel_focus: directory,
            actions: Vec::new(),
            selected: None,
            help: false,
            help_scroll: 0,
            detail: None,
            query: String::new(),
            searching: false,
            matches: Vec::new(),
            message: cap.reason.clone(),
            dirty: true,
            version: 0,
            watching: watch,
            watch: watcher,
            worker: Worker::new(cap.image, font),
            cap,
            pending: None,
            history: Vec::new(),
            quit: false,
        };
        if watch && app.watch.is_none() {
            app.message = "watch unavailable for this input; use r to reload".into();
        }
        app.replace(source, false);
        app
    }
    pub fn replace(&mut self, source: String, preserve: bool) {
        let anchor = preserve
            .then(|| self.layout.anchor(&self.doc, self.scroll))
            .flatten();
        let old: HashMap<_, _> = if preserve {
            self.diagrams
                .iter()
                .filter_map(|(i, s)| {
                    self.doc
                        .blocks
                        .get(*i)
                        .map(|b| (b.anchor.clone(), s.clone()))
                })
                .collect()
        } else {
            HashMap::new()
        };
        self.version += 1;
        self.source = source;
        self.doc = Document::parse(&self.source);
        self.diagrams.clear();
        self.actions.clear();
        self.selected = None;
        let mut jobs = Vec::new();
        for (ordinal, (i, anchor, source)) in self.doc.diagrams().into_iter().enumerate() {
            let previous = old.get(&anchor);
            let unchanged = previous
                .is_some_and(|s| s.source == source && s.good.is_some() && s.error.is_none());
            let state = DiagramState {
                source: source.clone(),
                good: previous.and_then(|s| s.good.clone()),
                error: if ordinal >= 32 {
                    Some("document limit: 32 rendered diagrams; source shown".into())
                } else {
                    None
                },
                pending: !unchanged && ordinal < 32,
                cached: unchanged,
            };
            if state.pending {
                jobs.push((i, anchor, source));
            }
            self.diagrams.insert(i, state);
        }
        for (i, b) in self.doc.blocks.iter().enumerate() {
            if b.kind == Kind::Diagram {
                self.actions.push(Action {
                    block: i,
                    target: None,
                    label: "Mermaid detail".into(),
                });
            }
            for (label, target) in &b.links {
                self.actions.push(Action {
                    block: i,
                    target: Some(target.clone()),
                    label: format!("{label} → {target}"),
                });
            }
        }
        self.pending = self
            .worker
            .submit(Job {
                version: self.version,
                diagrams: jobs,
            })
            .err();
        self.layout = Layout::build(
            &self.doc,
            &self.diagrams,
            self.width,
            self.cap.image,
            self.cap.cell,
        );
        self.scroll = if preserve {
            self.layout.restore(&self.doc, anchor.as_ref())
        } else {
            0
        };
        if !preserve {
            self.horizontal = 0;
            self.detail = None;
        }
        self.search_matches();
        self.dirty = true;
    }
    pub fn resize(&mut self, width: usize, height: usize) {
        if self.width != width || self.height != height {
            let anchor = self.layout.anchor(&self.doc, self.scroll);
            self.width = width.max(1);
            self.height = height.max(1);
            self.relayout();
            self.scroll = self.layout.restore(&self.doc, anchor.as_ref());
            self.dirty = true;
        }
    }
    fn relayout(&mut self) {
        let anchor = self.layout.anchor(&self.doc, self.scroll);
        self.layout = Layout::build(
            &self.doc,
            &self.diagrams,
            self.width,
            self.cap.image,
            self.cap.cell,
        );
        self.scroll = self.layout.restore(&self.doc, anchor.as_ref());
        self.search_matches();
    }
    pub fn apply(&mut self, c: Completion) -> bool {
        if c.version != self.version {
            return false;
        }
        let Some(state) = self.diagrams.get_mut(&c.block) else {
            return false;
        };
        if state.source != c.source || self.doc.blocks[c.block].anchor != c.anchor {
            return false;
        }
        state.pending = false;
        state.cached = c.cached;
        match c.result {
            Ok(d) => {
                state.good = Some(d);
                state.error = None;
            }
            Err(e) => state.error = Some(source::safe(&e)),
        }
        self.relayout();
        self.dirty = true;
        true
    }
    pub fn tick(&mut self) {
        if let Some(job) = self.pending.take() {
            self.pending = self.worker.submit(job).err();
        }
        while let Ok(c) = self.worker.rx.try_recv() {
            self.apply(c);
        }
        if self.watch.as_mut().is_some_and(Watch::due) {
            self.reload();
        }
        self.scroll = self.scroll.min(self.layout.rows.len().saturating_sub(1));
    }
    pub fn reload(&mut self) {
        if let Some(path) = &self.path {
            match source::read_file(path) {
                Ok(source) if source != self.source => {
                    self.message = "updated".into();
                    self.replace(source, true);
                }
                Ok(_) => {
                    if self.message.starts_with("read error:") {
                        self.message = "file available again".into();
                        self.dirty = true;
                    }
                }
                Err(e) => {
                    let msg = format!("read error: {e}; keeping current document");
                    if self.message != msg {
                        self.message = msg;
                        self.dirty = true;
                    }
                }
            }
        }
    }
    pub fn search_matches(&mut self) {
        let query = self.query.to_lowercase();
        self.matches.clear();
        if query.is_empty() {
            return;
        }
        for (i, row) in self.layout.rows.iter().enumerate() {
            if row.text().to_lowercase().contains(&query) {
                self.matches.push(i);
            }
        }
        if self.cap.image {
            for (i, b) in self.doc.blocks.iter().enumerate() {
                if b.kind == Kind::Diagram && b.text.to_lowercase().contains(&query) {
                    self.matches.push(self.layout.starts[i]);
                }
            }
        }
        self.matches.sort_unstable();
        self.matches.dedup();
    }
    fn next_match(&mut self, back: bool) {
        let next = if back {
            self.matches
                .iter()
                .rev()
                .find(|i| **i < self.scroll)
                .or(self.matches.last())
        } else {
            self.matches
                .iter()
                .find(|i| **i > self.scroll)
                .or(self.matches.first())
        };
        if let Some(i) = next {
            self.scroll = *i;
            self.horizontal = 0;
            self.selected = None;
        } else {
            self.message = "no matches".into();
        }
    }
    fn navigate(&mut self, path: PathBuf, fragment: Option<String>, remember: bool) -> Result<()> {
        let source = source::read_file(&path)?;
        if remember {
            self.history.push((
                self.path.clone(),
                self.source.clone(),
                self.scroll,
                self.horizontal,
            ));
            if self.history.len() > 32 {
                self.history.remove(0);
            }
        }
        self.path = Some(path.clone());
        self.replace(source, false);
        self.panel_focus = false;
        self.message = "opened".into();
        if self.watching {
            match &mut self.watch {
                Some(w) => {
                    if let Err(e) = w.retarget(&path) {
                        self.message = format!("watch error: {e}");
                        self.watch = None;
                    }
                }
                None => {
                    self.watch = Watch::new(&path).ok();
                }
            }
        }
        if let Some(anchor) = fragment {
            if let Some(i) = self.doc.blocks.iter().position(|b| b.anchor == anchor) {
                self.scroll = self.layout.starts[i];
            } else {
                self.message = format!("heading not found: {anchor}");
            }
        }
        Ok(())
    }
    fn enter(&mut self) -> Result<()> {
        if self.panel_focus {
            match self.panel {
                Some(Panel::Files) => {
                    if let Some(path) = self.files.get(self.panel_index).cloned() {
                        self.navigate(path, None, true)?;
                    }
                }
                Some(Panel::Toc) => {
                    let headings = self.headings();
                    if let Some((block, _)) = headings.get(self.panel_index) {
                        self.scroll = self.layout.starts[*block];
                        self.panel_focus = false;
                    }
                }
                None => {}
            }
            return Ok(());
        }
        let selection = self.selected.or_else(|| {
            self.actions.iter().position(|a| {
                self.layout.starts[a.block] >= self.scroll
                    && self.layout.starts[a.block] < self.scroll + self.height
            })
        });
        let action = selection
            .and_then(|s| self.actions.get(s))
            .cloned()
            .context("Tab selects a link or diagram; Enter opens it")?;
        if let Some(target) = action.target {
            let (path, anchor) = source::resolve_link(self.path.as_deref(), &target)?;
            self.navigate(path, anchor, true)?;
        } else {
            self.detail = Some(Detail {
                block: action.block,
                x: 0,
                y: 0,
                zoom: 1.,
            });
        }
        Ok(())
    }
    pub fn headings(&self) -> Vec<(usize, String)> {
        self.doc
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| match b.kind {
                Kind::Heading(level) => Some((
                    i,
                    format!(
                        "{}{}",
                        "  ".repeat(level.saturating_sub(1) as usize),
                        b.text
                    ),
                )),
                _ => None,
            })
            .collect()
    }
    pub fn key(&mut self, key: KeyEvent) {
        self.dirty = true;
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.searching {
            match key.code {
                KeyCode::Esc => self.searching = false,
                KeyCode::Enter => {
                    self.searching = false;
                    self.next_match(false);
                }
                KeyCode::Backspace => {
                    self.query.pop();
                    self.search_matches();
                }
                KeyCode::Char(c) if self.query.len() < 512 => {
                    self.query.push(c);
                    self.search_matches();
                }
                _ => {}
            }
            return;
        }
        if self.help {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?' | 'q') => self.help = false,
                KeyCode::Down | KeyCode::Char('j') => {
                    self.help_scroll = (self.help_scroll + 1).min(40)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.help_scroll = self.help_scroll.saturating_sub(1)
                }
                KeyCode::PageDown => self.help_scroll = (self.help_scroll + self.height).min(40),
                KeyCode::PageUp => self.help_scroll = self.help_scroll.saturating_sub(self.height),
                _ => {}
            }
            return;
        }
        if let Some(detail) = &mut self.detail {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => self.detail = None,
                KeyCode::Char('+' | '=') => detail.zoom = (detail.zoom * 1.25).min(8.),
                KeyCode::Char('-') => detail.zoom = (detail.zoom / 1.25).max(0.25),
                KeyCode::Char('0') => {
                    detail.zoom = 1.;
                    detail.x = 0;
                    detail.y = 0;
                }
                KeyCode::Left | KeyCode::Char('h') => detail.x = detail.x.saturating_sub(4),
                KeyCode::Right | KeyCode::Char('l') => detail.x = (detail.x + 4).min(8192),
                KeyCode::Up | KeyCode::Char('k') => detail.y = detail.y.saturating_sub(2),
                KeyCode::Down | KeyCode::Char('j') => detail.y = (detail.y + 2).min(8192),
                KeyCode::PageDown => detail.y = (detail.y + self.height).min(8192),
                KeyCode::PageUp => detail.y = detail.y.saturating_sub(self.height),
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.help = true,
            KeyCode::Char('f' | 't') => {
                let panel = if key.code == KeyCode::Char('f') {
                    Panel::Files
                } else {
                    Panel::Toc
                };
                if self.panel == Some(panel) {
                    self.panel = None;
                    self.panel_focus = false;
                } else {
                    if panel == Panel::Files {
                        self.files = source::markdown_files(&self.root);
                    }
                    self.panel = Some(panel);
                    self.panel_index = 0;
                    self.panel_focus = true;
                }
            }
            KeyCode::Esc => {
                self.panel = None;
                self.panel_focus = false;
                self.selected = None;
                self.query.clear();
                self.matches.clear();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.panel_focus {
                    let len = if self.panel == Some(Panel::Files) {
                        self.files.len()
                    } else {
                        self.headings().len()
                    };
                    self.panel_index = (self.panel_index + 1).min(len.saturating_sub(1));
                } else {
                    self.scroll = self.scroll.saturating_add(1);
                    self.selected = None;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.panel_focus {
                    self.panel_index = self.panel_index.saturating_sub(1);
                } else {
                    self.scroll = self.scroll.saturating_sub(1);
                    self.selected = None;
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.horizontal = self.horizontal.saturating_sub(4)
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.horizontal = (self.horizontal + 4).min(65536)
            }
            KeyCode::PageDown | KeyCode::Char(' ') => {
                self.scroll = self.scroll.saturating_add(self.height.saturating_sub(2));
                self.selected = None;
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(self.height.saturating_sub(2));
                self.selected = None;
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.scroll = 0;
                self.selected = None;
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.scroll = self.layout.rows.len().saturating_sub(self.height);
                self.selected = None;
            }
            KeyCode::Char('/') => {
                self.searching = true;
                self.query.clear();
                self.matches.clear();
            }
            KeyCode::Char('n') => self.next_match(false),
            KeyCode::Char('N') => self.next_match(true),
            KeyCode::Tab | KeyCode::BackTab => {
                self.panel_focus = false;
                if !self.actions.is_empty() {
                    let n = self.actions.len();
                    let i = if key.code == KeyCode::BackTab {
                        self.selected.map_or(n - 1, |s| (s + n - 1) % n)
                    } else {
                        self.selected.map_or_else(
                            || {
                                self.actions
                                    .iter()
                                    .position(|a| self.layout.starts[a.block] >= self.scroll)
                                    .unwrap_or(0)
                            },
                            |s| (s + 1) % n,
                        )
                    };
                    self.selected = Some(i);
                    self.scroll = self.layout.starts[self.actions[i].block];
                }
            }
            KeyCode::Enter => {
                if let Err(e) = self.enter() {
                    self.message = source::safe(&e.to_string());
                }
            }
            KeyCode::Char('b') | KeyCode::Backspace => {
                if let Some((path, source, scroll, horizontal)) = self.history.pop() {
                    self.path = path;
                    self.replace(source, false);
                    self.scroll = scroll;
                    self.horizontal = horizontal;
                    if let (Some(w), Some(path)) = (&mut self.watch, &self.path) {
                        let _ = w.retarget(path);
                    }
                }
            }
            KeyCode::Char('r') => self.reload(),
            _ => {}
        }
        self.scroll = self.scroll.min(self.layout.rows.len().saturating_sub(1));
    }
}
