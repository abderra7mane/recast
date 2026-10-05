//! Screenshots: pick a target, capture it, copy and save it, show the thumbnail.

pub mod beautify;
mod clipboard;
pub mod files;
mod thumbnail;
pub mod thumbnail_layout;

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use objc2::MainThreadMarker;
use recast_capture::{ScreenCapture, Screenshot, picker::PickMode};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};

pub use clipboard::copy_png;
use thumbnail::{Action, Thumbnail};

use crate::{
    picker,
    settings::{ScreenshotSettings, SettingsStore},
};

/// Unsaved captures stay in the cache this long, for drag and drop and Beautify.
const CACHE_AGE: Duration = Duration::from_secs(24 * 60 * 60);

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A screenshot kept in memory while its thumbnail or Beautify window is open.
pub struct Capture {
    pub id: u64,
    /// File name without the extension.
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    /// Straight RGBA.
    pub rgba: Vec<u8>,
    pub png: Vec<u8>,
    /// The PNG on disk: the saved file, or a copy in the cache.
    pub file: PathBuf,
    saved: Mutex<Option<PathBuf>>,
}

impl Capture {
    /// Encodes `shot` and writes it to `screenshots_dir` when saving is on, or else to
    /// `cache_dir`, where older unsaved captures are removed.
    pub fn store(
        shot: Screenshot,
        name: String,
        save_to_disk: bool,
        screenshots_dir: &Path,
        cache_dir: &Path,
    ) -> Result<Self, String> {
        let png = recast_render::bitmap::encode_png(shot.width, shot.height, &shot.rgba)
            .map_err(|e| e.to_string())?;
        let (file, saved) = if save_to_disk {
            let path = files::save_png(screenshots_dir, &name, &png)
                .map_err(|e| format!("cannot save to {}: {e}", screenshots_dir.display()))?;
            (path.clone(), Some(path))
        } else {
            files::prune(cache_dir, CACHE_AGE);
            let path = files::save_png(cache_dir, &name, &png).map_err(|e| e.to_string())?;
            (path, None)
        };
        Ok(Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            name,
            width: shot.width,
            height: shot.height,
            scale_factor: shot.scale_factor,
            rgba: shot.rgba,
            png,
            file,
            saved: Mutex::new(saved),
        })
    }

    pub fn saved_path(&self) -> Option<PathBuf> {
        self.saved.lock().expect("saved lock").clone()
    }

    /// Saves to `dir` unless it is saved already; returns the saved file.
    pub fn save(&self, dir: &Path) -> Result<PathBuf, String> {
        let mut saved = self.saved.lock().map_err(|e| e.to_string())?;
        if let Some(path) = saved.as_ref().filter(|p| p.exists()) {
            return Ok(path.clone());
        }
        let path = files::save_png(dir, &self.name, &self.png)
            .map_err(|e| format!("cannot save to {}: {e}", dir.display()))?;
        *saved = Some(path.clone());
        Ok(path)
    }

    /// Size in points.
    pub fn points(&self) -> (f64, f64) {
        let scale = self.scale_factor.max(1.0);
        (self.width as f64 / scale, self.height as f64 / scale)
    }
}

pub fn reveal(path: &Path) {
    if let Err(e) = std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(path)
        .status()
    {
        log::warn!("cannot reveal {}: {e}", path.display());
    }
}

fn cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("screenshots"))
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotTaken {
    /// The saved file, when saving is on.
    pub path: Option<String>,
    pub width: u32,
    pub height: u32,
    pub copied: bool,
    /// Problems that didn't stop the capture, such as a failed clipboard copy.
    pub warnings: Vec<String>,
}

fn on_action(app: AppHandle, capture: Arc<Capture>) -> thumbnail::OnAction {
    Box::new(move |action| {
        match action {
            Action::Copy => {
                if let Err(e) = copy_png(&capture.png) {
                    log::warn!("{e}");
                }
            }
            Action::Save => match capture.save(&files::screenshots_dir()) {
                Ok(path) => reveal(&path),
                Err(e) => log::warn!("{e}"),
            },
            Action::Beautify => {
                let (app, capture) = (app.clone(), capture.clone());
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = beautify::open_window(&app, capture) {
                        log::warn!("cannot open Beautify: {e}");
                    }
                });
            }
            Action::Close | Action::DraggedOut => {}
        }
        let mtm = MainThreadMarker::new().expect("thumbnail actions run on the main thread");
        thumbnail::close(Some(capture.id), mtm);
    })
}

async fn capture(
    app: &AppHandle,
    options: ScreenshotSettings,
    mode: PickMode,
) -> Result<Option<ScreenshotTaken>, String> {
    let Some(picked) = picker::pick(app, mode).await? else {
        return Ok(None);
    };
    let cache = cache_dir(app)?;
    let target = picked.target.clone();
    let save = options.save_to_disk;
    let capture = tauri::async_runtime::spawn_blocking(move || {
        let shot = recast_capture::platform()
            .screenshot(&target)
            .map_err(|e| e.to_string())?;
        let name = files::capture_name(chrono::Local::now());
        Capture::store(shot, name, save, &files::screenshots_dir(), &cache)
    })
    .await
    .map_err(|e| e.to_string())??;
    let capture = Arc::new(capture);

    let mut warnings = Vec::new();
    let copied = options.copy_to_clipboard
        && copy_png(&capture.png)
            .inspect_err(|e| warnings.push(e.clone()))
            .is_ok();

    let shown = Thumbnail {
        id: capture.id,
        png: capture.png.clone(),
        size: capture.points(),
        file: capture.file.clone(),
        display_id: picked.display_id,
    };
    let handler_app = app.clone();
    let handler_capture = capture.clone();
    app.run_on_main_thread(move || {
        let mtm = MainThreadMarker::new().expect("runs on the main thread");
        thumbnail::show(mtm, shown, on_action(handler_app, handler_capture));
    })
    .map_err(|e| e.to_string())?;

    Ok(Some(ScreenshotTaken {
        path: capture.saved_path().map(|p| p.display().to_string()),
        width: capture.width,
        height: capture.height,
        copied,
        warnings,
    }))
}

/// Picks a target, captures it and shows the thumbnail; `None` when the user cancels.
#[tauri::command]
#[specta::specta]
pub async fn take_screenshot(
    app: AppHandle,
    settings: State<'_, SettingsStore>,
    mode: PickMode,
) -> Result<Option<ScreenshotTaken>, String> {
    let options = settings.get().screenshots;
    let main = picker::hide_main(&app);
    let taken = capture(&app, options, mode).await;
    picker::show_main(main);
    taken
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shot() -> Screenshot {
        Screenshot {
            width: 4,
            height: 2,
            scale_factor: 2.0,
            rgba: (0..32).map(|i| (i * 8) as u8).collect(),
        }
    }

    #[test]
    fn saved_captures_go_to_the_screenshots_folder() {
        let dir = tempfile::tempdir().unwrap();
        let (pictures, cache) = (dir.path().join("Pictures"), dir.path().join("cache"));
        let capture = Capture::store(shot(), "Recast x".into(), true, &pictures, &cache).unwrap();
        assert_eq!(capture.file, pictures.join("Recast x.png"));
        assert_eq!(capture.saved_path(), Some(capture.file.clone()));
        assert_eq!(capture.points(), (2.0, 1.0));
        let (w, h, pixels) = recast_render::bitmap::load_png(&capture.file).unwrap();
        assert_eq!((w, h, pixels), (4, 2, shot().rgba));
        assert_eq!(capture.save(&pictures).unwrap(), capture.file);
        assert!(!cache.exists());
    }

    #[test]
    fn unsaved_captures_wait_in_the_cache_until_saved() {
        let dir = tempfile::tempdir().unwrap();
        let (pictures, cache) = (dir.path().join("Pictures"), dir.path().join("cache"));
        let capture = Capture::store(shot(), "Recast y".into(), false, &pictures, &cache).unwrap();
        assert_eq!(capture.file, cache.join("Recast y.png"));
        assert_eq!(capture.saved_path(), None);
        assert!(!pictures.exists());

        let saved = capture.save(&pictures).unwrap();
        assert_eq!(saved, pictures.join("Recast y.png"));
        assert_eq!(capture.save(&pictures).unwrap(), saved, "saved once");
        assert_eq!(std::fs::read_dir(&pictures).unwrap().count(), 1);
    }
}
