use anyhow::Result;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

pub struct Watch {
    watcher: RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
    parent: PathBuf,
    target: PathBuf,
    pending: Option<Instant>,
    last_poll: Instant,
}
impl Watch {
    pub fn new(path: &Path) -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |e| {
            let _ = tx.send(e);
        })?;
        let parent = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        watcher.watch(&parent, RecursiveMode::NonRecursive)?;
        Ok(Self {
            watcher,
            rx,
            parent,
            target: path.to_path_buf(),
            pending: None,
            last_poll: Instant::now(),
        })
    }
    pub fn retarget(&mut self, path: &Path) -> Result<()> {
        let parent = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        if self.parent != parent {
            self.watcher.watch(&parent, RecursiveMode::NonRecursive)?;
            let _ = self.watcher.unwatch(&self.parent);
            self.parent = parent;
        }
        self.target = path.to_owned();
        self.pending = None;
        Ok(())
    }
    /// Parent-directory events catch rename-over saves. Periodic reconciliation handles lost events.
    pub fn due(&mut self) -> bool {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Ok(e)
                    if !matches!(e.kind, notify::EventKind::Access(_))
                        && (e.paths.is_empty() || e.paths.iter().any(|p| p == &self.target)) =>
                {
                    self.pending = Some(Instant::now())
                }
                Err(_) => self.pending = Some(Instant::now()),
                _ => {}
            }
        }
        if self
            .pending
            .is_some_and(|t| t.elapsed() >= Duration::from_millis(180))
        {
            self.pending = None;
            self.last_poll = Instant::now();
            return true;
        }
        if self.pending.is_none() && self.last_poll.elapsed() >= Duration::from_secs(1) {
            self.last_poll = Instant::now();
            return true;
        }
        false
    }
}
