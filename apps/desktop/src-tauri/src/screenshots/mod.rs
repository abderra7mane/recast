//! Screenshots: pick a target, capture it, copy and save it, show the thumbnail.

mod clipboard;
pub mod files;
pub mod markup;
mod sound;
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
use tauri::{AppHandle, Manager};

pub use clipboard::{copy_png, copy_text};
use thumbnail::{Action, Thumbnail};

use crate::{
    picker,
    settings::{ScreenshotSettings, SettingsStore},
};

/// Unsaved captures stay in the cache this long, for drag and drop and markup.
const CACHE_AGE: Duration = Duration::from_secs(24 * 60 * 60);

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A screenshot kept in memory while its thumbnail or markup window is open.
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

    /// A PNG file to edit, `scale_factor` pixels per point; it counts as saved.
    pub fn open(path: &Path, scale_factor: f64) -> Result<Self, String> {
        let (width, height, rgba) =
            recast_render::bitmap::load_png(path).map_err(|e| e.to_string())?;
        let png = std::fs::read(path).map_err(|e| e.to_string())?;
        Ok(Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            name: path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            width,
            height,
            scale_factor,
            rgba,
            png,
            file: path.to_path_buf(),
            saved: Mutex::new(Some(path.to_path_buf())),
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

    /// The file the user sees: the saved one, or else the cached copy.
    pub fn current_file(&self) -> PathBuf {
        self.saved_path()
            .filter(|p| p.exists())
            .unwrap_or_else(|| self.file.clone())
    }

    /// Moves the capture's file to the Trash with `trash`: the saved file when there is
    /// one, whose cached copy is removed, or else the cached file.
    pub fn delete(&self, trash: impl Fn(&Path) -> Result<(), String>) -> Result<(), String> {
        let mut saved = self.saved.lock().map_err(|e| e.to_string())?;
        match saved.clone().filter(|p| p.exists()) {
            Some(path) => {
                trash(&path)?;
                *saved = None;
                if self.file != path {
                    let _ = std::fs::remove_file(&self.file);
                }
                Ok(())
            }
            None => trash(&self.file),
        }
    }

    /// Size in points.
    pub fn points(&self) -> (f64, f64) {
        let scale = self.scale_factor.max(1.0);
        (self.width as f64 / scale, self.height as f64 / scale)
    }
}

pub fn move_to_trash(path: &Path) -> Result<(), String> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|e| e.localizedDescription().to_string())
}

/// Opens a screenshot for editing.
pub fn edit(app: &AppHandle, capture: Arc<Capture>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = markup::open_window(&app, capture) {
            log::warn!("cannot open the screenshot editor: {e}");
        }
    });
}

/// Opens the saved screenshot at `path` in the markup editor, or focuses the editor
/// already showing it.
pub fn edit_saved(app: &AppHandle, path: &Path) -> Result<(), String> {
    if markup::focus_window_for(app, path) {
        return Ok(());
    }
    let scale_factor = app
        .primary_monitor()
        .ok()
        .flatten()
        .map_or(2.0, |monitor| monitor.scale_factor());
    let capture = Capture::open(path, scale_factor)?;
    markup::open_window(app, Arc::new(capture))
}

#[tauri::command]
#[specta::specta]
pub async fn list_screenshots(
    settings: tauri::State<'_, SettingsStore>,
) -> Result<Vec<files::ScreenshotSummary>, String> {
    Ok(files::list_screenshots(&settings.get().screenshots.dir()))
}

#[tauri::command]
#[specta::specta]
pub async fn edit_screenshot(app: AppHandle, path: String) -> Result<(), String> {
    edit_saved(&app, Path::new(&path))
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

/// What thumbnail actions reach outside the capture.
trait Effects {
    fn copy(&self, png: &[u8]) -> Result<(), String>;
    fn reveal(&self, path: &Path);
    fn edit(&self, capture: Arc<Capture>);
    fn trash(&self, path: &Path) -> Result<(), String>;
}

struct AppEffects(AppHandle);

impl Effects for AppEffects {
    fn copy(&self, png: &[u8]) -> Result<(), String> {
        copy_png(png)
    }

    fn reveal(&self, path: &Path) {
        reveal(path);
    }

    fn edit(&self, capture: Arc<Capture>) {
        edit(&self.0, capture);
    }

    fn trash(&self, path: &Path) -> Result<(), String> {
        move_to_trash(path)
    }
}

/// Carries out a thumbnail action on `capture`; the thumbnail closes afterwards.
fn perform(action: Action, capture: &Arc<Capture>, dir: &Path, effects: &impl Effects) {
    let done = match action {
        Action::Copy => effects.copy(&capture.png),
        Action::Save => capture.save(dir).map(|path| effects.reveal(&path)),
        Action::ShowInFinder => {
            effects.reveal(&capture.current_file());
            Ok(())
        }
        Action::Edit => {
            effects.edit(capture.clone());
            Ok(())
        }
        Action::Delete => capture
            .delete(|path| effects.trash(path))
            .map_err(|e| format!("cannot move the screenshot to the Trash: {e}")),
        Action::Close | Action::DraggedOut => Ok(()),
    };
    if let Err(e) = done {
        log::warn!("{e}");
    }
}

fn on_action(app: AppHandle, capture: Arc<Capture>, dir: PathBuf) -> thumbnail::OnAction {
    let effects = AppEffects(app);
    Box::new(move |action| {
        perform(action, &capture, &dir, &effects);
        let mtm = MainThreadMarker::new().expect("thumbnail actions run on the main thread");
        thumbnail::close(Some(capture.id), mtm);
    })
}

async fn capture(
    app: &AppHandle,
    options: ScreenshotSettings,
    mode: PickMode,
) -> Result<Option<ScreenshotTaken>, String> {
    let Some(picker::Pick { picked, focus }) = picker::pick(
        app,
        picker::PickRequest::new(mode, picker::Purpose::Capture),
    )
    .await?
    else {
        return Ok(None);
    };
    focus.restore_from(app);
    let cache = cache_dir(app)?;
    let target = picked.target.clone();
    let save = options.save_to_disk;
    let shutter = options.play_shutter_sound;
    let dir = options.dir();
    let save_dir = dir.clone();
    let capture = tauri::async_runtime::spawn_blocking(move || {
        let shot = recast_capture::platform()
            .screenshot(&target)
            .map_err(|e| e.to_string())?;
        if shutter {
            sound::play_shutter();
        }
        let name = files::capture_name(chrono::Local::now());
        Capture::store(shot, name, save, &save_dir, &cache)
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
        thumbnail::show(mtm, shown, on_action(handler_app, handler_capture, dir));
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
pub async fn take(app: &AppHandle, mode: PickMode) -> Result<Option<ScreenshotTaken>, String> {
    let options = app.state::<SettingsStore>().get().screenshots;
    let library = picker::hide_library(app);
    let taken = capture(app, options, mode).await;
    picker::show_library(library);
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

    #[derive(Default)]
    struct Recorded {
        copied: std::cell::RefCell<Vec<usize>>,
        revealed: std::cell::RefCell<Vec<PathBuf>>,
        edited: std::cell::RefCell<Vec<u64>>,
        trashed: std::cell::RefCell<Vec<PathBuf>>,
    }

    impl Effects for Recorded {
        fn copy(&self, png: &[u8]) -> Result<(), String> {
            self.copied.borrow_mut().push(png.len());
            Ok(())
        }

        fn reveal(&self, path: &Path) {
            self.revealed.borrow_mut().push(path.to_path_buf());
        }

        fn edit(&self, capture: Arc<Capture>) {
            self.edited.borrow_mut().push(capture.id);
        }

        fn trash(&self, path: &Path) -> Result<(), String> {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
            self.trashed.borrow_mut().push(path.to_path_buf());
            Ok(())
        }
    }

    #[test]
    fn menu_actions_act_on_the_capture() {
        let dir = tempfile::tempdir().unwrap();
        let (pictures, cache) = (dir.path().join("Pictures"), dir.path().join("cache"));
        let capture =
            Arc::new(Capture::store(shot(), "Recast m".into(), false, &pictures, &cache).unwrap());
        let cached = cache.join("Recast m.png");
        let saved = pictures.join("Recast m.png");
        let effects = Recorded::default();

        perform(Action::Copy, &capture, &pictures, &effects);
        assert_eq!(*effects.copied.borrow(), [capture.png.len()]);

        perform(Action::ShowInFinder, &capture, &pictures, &effects);
        perform(Action::Save, &capture, &pictures, &effects);
        perform(Action::ShowInFinder, &capture, &pictures, &effects);
        assert_eq!(
            *effects.revealed.borrow(),
            [cached.clone(), saved.clone(), saved.clone()]
        );

        perform(Action::Edit, &capture, &pictures, &effects);
        assert_eq!(*effects.edited.borrow(), [capture.id]);

        perform(Action::Close, &capture, &pictures, &effects);
        perform(Action::DraggedOut, &capture, &pictures, &effects);
        assert!(effects.trashed.borrow().is_empty());
        assert!(saved.exists() && cached.exists());

        perform(Action::Delete, &capture, &pictures, &effects);
        assert_eq!(*effects.trashed.borrow(), std::slice::from_ref(&saved));
        assert!(!saved.exists() && !cached.exists());
    }

    /// A Trash that records what was moved to it.
    fn trash(moved: &std::cell::RefCell<Vec<PathBuf>>) -> impl Fn(&Path) -> Result<(), String> {
        |path| {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
            moved.borrow_mut().push(path.to_path_buf());
            Ok(())
        }
    }

    #[test]
    fn delete_trashes_the_saved_file() {
        let dir = tempfile::tempdir().unwrap();
        let (pictures, cache) = (dir.path().join("Pictures"), dir.path().join("cache"));
        std::fs::create_dir_all(&pictures).unwrap();
        std::fs::write(pictures.join("Recast d.png"), b"older").unwrap();
        let capture = Capture::store(shot(), "Recast d".into(), true, &pictures, &cache).unwrap();
        assert_eq!(capture.current_file(), pictures.join("Recast d (2).png"));

        let moved = std::cell::RefCell::new(Vec::new());
        capture.delete(trash(&moved)).unwrap();
        assert_eq!(*moved.borrow(), [pictures.join("Recast d (2).png")]);
        assert!(pictures.join("Recast d.png").exists(), "other files stay");
        assert_eq!(capture.saved_path(), None);
    }

    #[test]
    fn delete_trashes_the_cached_file_when_unsaved() {
        let dir = tempfile::tempdir().unwrap();
        let (pictures, cache) = (dir.path().join("Pictures"), dir.path().join("cache"));
        let capture = Capture::store(shot(), "Recast e".into(), false, &pictures, &cache).unwrap();
        assert_eq!(capture.current_file(), cache.join("Recast e.png"));

        let moved = std::cell::RefCell::new(Vec::new());
        capture.delete(trash(&moved)).unwrap();
        assert_eq!(*moved.borrow(), [cache.join("Recast e.png")]);
    }

    #[test]
    fn delete_after_saving_trashes_the_saved_file_and_drops_the_cached_copy() {
        let dir = tempfile::tempdir().unwrap();
        let (pictures, cache) = (dir.path().join("Pictures"), dir.path().join("cache"));
        let capture = Capture::store(shot(), "Recast f".into(), false, &pictures, &cache).unwrap();
        let saved = capture.save(&pictures).unwrap();
        assert_eq!(capture.current_file(), saved);

        let moved = std::cell::RefCell::new(Vec::new());
        capture.delete(trash(&moved)).unwrap();
        assert_eq!(*moved.borrow(), [saved]);
        assert!(!cache.join("Recast f.png").exists());
    }

    #[test]
    fn a_failed_delete_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let (pictures, cache) = (dir.path().join("Pictures"), dir.path().join("cache"));
        let capture = Capture::store(shot(), "Recast g".into(), false, &pictures, &cache).unwrap();
        let refuse = |_: &Path| Err("the Trash is not available".to_string());
        assert!(capture.delete(refuse).is_err());
        assert!(cache.join("Recast g.png").exists());
    }
}
