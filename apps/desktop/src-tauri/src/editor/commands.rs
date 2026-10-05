//! Tauri commands and events of the editor windows. Each editor window has a label
//! `editor-N` and at most one session.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use recast_export::{ExportRequest, Progress};
use recast_project::{EditSettings, ZoomSegment};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_specta::Event;

use super::{EditorInit, EditorSession, EditorStatus, PreviewStats, ProjectSummary};
use crate::settings::SettingsStore;

pub const LABEL_PREFIX: &str = "editor-";
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

struct EditorWindow {
    path: PathBuf,
    session: Option<Arc<EditorSession>>,
    export: Option<Arc<AtomicBool>>,
}

#[derive(Default)]
pub struct Editors {
    windows: Mutex<HashMap<String, EditorWindow>>,
    next_id: AtomicU64,
    running_exports: AtomicUsize,
}

/// Longest wait at quit for cancelled exports to remove their partial files.
const QUIT_EXPORT_WAIT: Duration = Duration::from_secs(5);

impl Editors {
    /// Before the app quits: writes pending edits of every open project and
    /// cancels running exports, waiting for them to remove their partial files.
    pub fn shutdown(&self) {
        let Ok(windows) = self.windows.lock() else {
            return;
        };
        let sessions: Vec<_> = windows.values().filter_map(|w| w.session.clone()).collect();
        for cancel in windows.values().filter_map(|w| w.export.as_ref()) {
            cancel.store(true, Ordering::SeqCst);
        }
        drop(windows);
        for session in sessions {
            if let Err(e) = session.flush() {
                log::warn!("cannot save {}: {e}", session.path().display());
            }
        }
        let started = Instant::now();
        while self.running_exports.load(Ordering::SeqCst) > 0
            && started.elapsed() < QUIT_EXPORT_WAIT
        {
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn session(&self, label: &str) -> Result<Arc<EditorSession>, String> {
        self.windows
            .lock()
            .map_err(|e| e.to_string())?
            .get(label)
            .and_then(|w| w.session.clone())
            .ok_or_else(|| "the project is not open".to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub bundle_path: String,
    pub frame: u32,
    pub total_frames: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ExportFinished {
    pub bundle_path: String,
    pub output_path: String,
    pub cancelled: bool,
    pub error: Option<String>,
}

/// Opens an editor window for the bundle at `path`, or focuses the one already open.
pub fn open_editor_window(app: &AppHandle, path: &Path) -> Result<(), String> {
    let editors = app.state::<Editors>();
    let existing = editors
        .windows
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|(_, w)| w.path == path)
        .map(|(label, _)| label.clone());
    if let Some(window) = existing.and_then(|label| app.get_webview_window(&label)) {
        let _ = window.unminimize();
        return window.set_focus().map_err(|e| e.to_string());
    }

    let label = format!(
        "{LABEL_PREFIX}{}",
        editors.next_id.fetch_add(1, Ordering::Relaxed)
    );
    let title = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Recast".into());
    editors.windows.lock().map_err(|e| e.to_string())?.insert(
        label.clone(),
        EditorWindow {
            path: path.to_path_buf(),
            session: None,
            export: None,
        },
    );
    let window = crate::activation::build(
        WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
            .title(title)
            .inner_size(1440.0, 900.0)
            .min_inner_size(1040.0, 660.0)
            .theme(Some(tauri::Theme::Dark)),
    )?;
    let app = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            let removed = app
                .state::<Editors>()
                .windows
                .lock()
                .ok()
                .and_then(|mut windows| windows.remove(&label));
            if let Some(EditorWindow {
                session, export, ..
            }) = removed
            {
                if let Some(cancel) = export {
                    cancel.store(true, Ordering::SeqCst);
                }
                // Closing the last window ends the process right away, so the
                // edits are written before the session is dropped in the background.
                if let Some(session) = &session
                    && let Err(e) = session.flush()
                {
                    log::warn!("cannot save {}: {e}", session.path().display());
                }
                std::thread::spawn(move || drop(session));
            }
        }
    });
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn list_projects(
    settings: State<'_, SettingsStore>,
) -> Result<Vec<ProjectSummary>, String> {
    Ok(super::list_projects(&settings.get().recording.dir()))
}

#[tauri::command]
#[specta::specta]
pub async fn open_editor(app: AppHandle, path: String) -> Result<(), String> {
    open_editor_window(&app, Path::new(&path))
}

/// Starts the session of the calling editor window (or returns the running one).
#[tauri::command]
#[specta::specta]
pub async fn editor_open(
    window: WebviewWindow,
    editors: State<'_, Editors>,
) -> Result<EditorInit, String> {
    let label = window.label().to_string();
    let path = {
        let windows = editors.windows.lock().map_err(|e| e.to_string())?;
        let entry = windows.get(&label).ok_or("this window has no project")?;
        if let Some(session) = &entry.session {
            session.reset_sequences();
            return Ok(session.init());
        }
        entry.path.clone()
    };
    let session = tauri::async_runtime::spawn_blocking(move || EditorSession::open(&path))
        .await
        .map_err(|e| e.to_string())??;
    let session = Arc::new(session);
    let kept = {
        let mut windows = editors.windows.lock().map_err(|e| e.to_string())?;
        let entry = windows.get_mut(&label).ok_or("the window was closed")?;
        entry.session.get_or_insert(session.clone()).clone()
    };
    if !Arc::ptr_eq(&kept, &session) {
        // Another call opened the project first; this one's session is not used.
        std::thread::spawn(move || drop(session));
    }
    Ok(kept.init())
}

/// Applies edits to the preview and saves them; returns the auto zoom segments.
/// `seq` increases with every call; a call that arrives after a newer one is
/// ignored and returns `None`.
#[tauri::command]
#[specta::specta]
pub async fn editor_set_settings(
    window: WebviewWindow,
    editors: State<'_, Editors>,
    settings: EditSettings,
    seq: u32,
) -> Result<Option<Vec<ZoomSegment>>, String> {
    Ok(editors.session(window.label())?.set_settings(settings, seq))
}

#[tauri::command]
#[specta::specta]
pub async fn editor_play(window: WebviewWindow, editors: State<'_, Editors>) -> Result<(), String> {
    editors.session(window.label())?.play();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn editor_pause(
    window: WebviewWindow,
    editors: State<'_, Editors>,
) -> Result<(), String> {
    editors.session(window.label())?.pause();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn editor_seek(
    window: WebviewWindow,
    editors: State<'_, Editors>,
    t_ms: f64,
    seq: u32,
) -> Result<(), String> {
    editors.session(window.label())?.seek(t_ms, seq);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn editor_set_loop(
    window: WebviewWindow,
    editors: State<'_, Editors>,
    looping: bool,
) -> Result<(), String> {
    editors.session(window.label())?.set_loop(looping);
    Ok(())
}

/// The preview's drawing area in device pixels.
#[tauri::command]
#[specta::specta]
pub async fn editor_resize(
    window: WebviewWindow,
    editors: State<'_, Editors>,
    width: u32,
    height: u32,
) -> Result<(), String> {
    editors.session(window.label())?.resize(width, height);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn editor_status(
    window: WebviewWindow,
    editors: State<'_, Editors>,
) -> Result<EditorStatus, String> {
    Ok(editors.session(window.label())?.status())
}

#[tauri::command]
#[specta::specta]
pub async fn editor_stats(
    window: WebviewWindow,
    editors: State<'_, Editors>,
) -> Result<PreviewStats, String> {
    Ok(editors.session(window.label())?.stats())
}

/// Exports the project with its current edits to `output_path`. Progress and the
/// result arrive as `ExportProgress` and `ExportFinished` events.
#[tauri::command]
#[specta::specta]
pub async fn export_start(
    app: AppHandle,
    window: WebviewWindow,
    editors: State<'_, Editors>,
    output_path: String,
) -> Result<(), String> {
    let label = window.label().to_string();
    let session = editors.session(&label)?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut windows = editors.windows.lock().map_err(|e| e.to_string())?;
        let entry = windows.get_mut(&label).ok_or("the project is not open")?;
        if entry.export.is_some() {
            return Err("an export is already running".into());
        }
        entry.export = Some(cancel.clone());
    }
    session.pause();
    let _ = session.flush();
    let settings = session.settings();
    let bundle = session.path().to_path_buf();
    let bundle_path = bundle.display().to_string();
    let label_for_errors = label.clone();
    editors.running_exports.fetch_add(1, Ordering::SeqCst);
    let spawned = std::thread::Builder::new()
        .name("export".into())
        .spawn(move || {
            let output = PathBuf::from(&output_path);
            let mut last = Instant::now() - PROGRESS_INTERVAL;
            let mut on_progress = |p: Progress| {
                if last.elapsed() >= PROGRESS_INTERVAL || p.frame == p.total_frames {
                    last = Instant::now();
                    let _ = ExportProgress {
                        bundle_path: bundle_path.clone(),
                        frame: p.frame as u32,
                        total_frames: p.total_frames as u32,
                    }
                    .emit(&app);
                }
            };
            let result = recast_export::export(
                &ExportRequest {
                    bundle: &bundle,
                    output: &output,
                    settings: Some(settings),
                },
                &mut on_progress,
                &cancel,
            );
            let editors = app.state::<Editors>();
            if let Ok(mut windows) = editors.windows.lock()
                && let Some(entry) = windows.get_mut(&label)
            {
                entry.export = None;
            }
            editors.running_exports.fetch_sub(1, Ordering::SeqCst);
            let cancelled = matches!(result, Err(recast_export::Error::Cancelled));
            let _ = ExportFinished {
                bundle_path,
                output_path,
                cancelled,
                error: result.err().filter(|_| !cancelled).map(|e| e.to_string()),
            }
            .emit(&app);
        });
    if let Err(e) = spawned {
        editors.running_exports.fetch_sub(1, Ordering::SeqCst);
        if let Some(entry) = editors
            .windows
            .lock()
            .map_err(|e| e.to_string())?
            .get_mut(&label_for_errors)
        {
            entry.export = None;
        }
        return Err(e.to_string());
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn export_cancel(
    window: WebviewWindow,
    editors: State<'_, Editors>,
) -> Result<(), String> {
    if let Some(cancel) = editors
        .windows
        .lock()
        .map_err(|e| e.to_string())?
        .get(window.label())
        .and_then(|w| w.export.clone())
    {
        cancel.store(true, Ordering::SeqCst);
    }
    Ok(())
}

#[cfg(all(test, feature = "synthetic"))]
mod tests {
    use super::*;
    use crate::editor::test_support;

    #[test]
    fn shutdown_saves_pending_edits_and_cancels_exports() {
        let dir = tempfile::tempdir().unwrap();
        let path = test_support::fixture(dir.path());
        let session = Arc::new(EditorSession::open(&path).unwrap());
        let cancel = Arc::new(AtomicBool::new(false));
        let editors = Editors::default();
        editors.windows.lock().unwrap().insert(
            "editor-0".into(),
            EditorWindow {
                path: path.clone(),
                session: Some(session.clone()),
                export: Some(cancel.clone()),
            },
        );
        let mut settings = session.settings();
        settings.cursor.size = 3.5;
        session.set_settings(settings, 1).unwrap();

        editors.shutdown();
        assert!(cancel.load(Ordering::SeqCst));
        let saved = recast_project::Bundle::open(&path)
            .unwrap()
            .load_project()
            .unwrap();
        assert_eq!(saved.edits.cursor.size, 3.5);
    }
}
