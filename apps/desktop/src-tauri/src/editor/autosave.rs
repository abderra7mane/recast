//! Saves `project.json` in the background once edits pause, so typing or dragging
//! does not write on every change. `Bundle::save_project` replaces the file
//! atomically.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use recast_project::{Bundle, Project};

/// Edits keep arriving for at most this long before a save happens anyway.
const MAX_DELAY: Duration = Duration::from_secs(5);

enum Message {
    Save(Box<Project>),
    Flush(mpsc::Sender<Result<(), String>>),
}

pub struct Autosave {
    tx: Option<mpsc::Sender<Message>>,
    writes: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

impl Autosave {
    pub fn start(bundle: Bundle, delay: Duration) -> Self {
        let (tx, rx) = mpsc::channel();
        let writes = Arc::new(AtomicU64::new(0));
        let thread = {
            let writes = writes.clone();
            thread::Builder::new()
                .name("autosave".into())
                .spawn(move || run(&bundle, delay, &rx, &writes))
                .ok()
        };
        Self {
            tx: Some(tx),
            writes,
            thread,
        }
    }

    pub fn save(&self, project: Project) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Message::Save(Box::new(project)));
        }
    }

    /// Writes any pending save now.
    pub fn flush(&self) -> Result<(), String> {
        let (reply, done) = mpsc::channel();
        self.tx
            .as_ref()
            .ok_or("autosave stopped")?
            .send(Message::Flush(reply))
            .map_err(|_| "autosave stopped".to_string())?;
        done.recv().map_err(|_| "autosave stopped".to_string())?
    }

    /// How many times the file was written.
    pub fn writes(&self) -> u64 {
        self.writes.load(Ordering::SeqCst)
    }
}

impl Drop for Autosave {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(bundle: &Bundle, delay: Duration, rx: &mpsc::Receiver<Message>, writes: &AtomicU64) {
    let mut pending: Option<(Box<Project>, Instant)> = None;
    let write = |pending: &mut Option<(Box<Project>, Instant)>| -> Result<(), String> {
        let Some((project, _)) = pending.take() else {
            return Ok(());
        };
        bundle.save_project(&project).map_err(|e| {
            log::warn!("autosave failed: {e}");
            e.to_string()
        })?;
        writes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    };
    loop {
        let message = match &pending {
            Some((_, since)) => {
                let wait = delay.min(MAX_DELAY.saturating_sub(since.elapsed()));
                rx.recv_timeout(wait)
            }
            None => rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected),
        };
        match message {
            Ok(Message::Save(project)) => {
                let since = pending.take().map_or_else(Instant::now, |(_, since)| since);
                pending = Some((project, since));
            }
            Ok(Message::Flush(reply)) => {
                let _ = reply.send(write(&mut pending));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = write(&mut pending);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = write(&mut pending);
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use recast_project::EditSettings;

    use super::*;
    use crate::editor::test_support;

    fn bundle(dir: &std::path::Path) -> (Bundle, Project) {
        let project = test_support::project("Take", 0.0);
        let path = dir.join("Take.recast");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(
            path.join(recast_project::PROJECT_FILE),
            serde_json::to_vec(&project).unwrap(),
        )
        .unwrap();
        (Bundle::open(&path).unwrap(), project)
    }

    #[test]
    fn saves_once_edits_pause_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let (bundle, mut project) = bundle(dir.path());
        let autosave = Autosave::start(bundle.clone(), Duration::from_millis(80));
        for level in [2.5, 3.0, 3.5] {
            project.edits.zoom.level = level;
            project.edits.background.padding = level / 10.0;
            autosave.save(project.clone());
        }
        assert_eq!(autosave.writes(), 0);
        thread::sleep(Duration::from_millis(400));
        assert_eq!(autosave.writes(), 1);
        let saved = bundle.load_project().unwrap();
        assert_eq!(saved, project);
        assert_eq!(saved.edits.zoom.level, 3.5);
        assert!(!bundle.file("project.tmp").exists());
    }

    #[test]
    fn flush_and_drop_write_pending_edits() {
        let dir = tempfile::tempdir().unwrap();
        let (bundle, mut project) = bundle(dir.path());
        let autosave = Autosave::start(bundle.clone(), Duration::from_secs(60));
        project.edits = EditSettings {
            trim: recast_project::Trim {
                start_ms: 100.0,
                end_ms: Some(900.0),
            },
            ..Default::default()
        };
        autosave.save(project.clone());
        autosave.flush().unwrap();
        assert_eq!(autosave.writes(), 1);
        assert_eq!(bundle.load_project().unwrap(), project);
        autosave.flush().unwrap();
        assert_eq!(autosave.writes(), 1);

        project.edits.cursor.size = 3.0;
        autosave.save(project.clone());
        drop(autosave);
        assert_eq!(bundle.load_project().unwrap().edits.cursor.size, 3.0);
    }
}
