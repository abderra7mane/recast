//! Beautify windows: a screenshot on a background with padding, rounded corners and a
//! shadow, rendered by the recording compositor. Each window has a label `beautify-N`.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use base64::Engine;
use recast_project::{BackgroundFill, BackgroundSettings};
use recast_render::{bitmap, still::StillRenderer};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::{Capture, copy_png, files};
use crate::settings::SettingsStore;

pub const LABEL_PREFIX: &str = "beautify-";
/// Largest output side; the GPU's texture limit on Apple Silicon.
const MAX_SIDE: u32 = 16_384;

struct Session {
    capture: Arc<Capture>,
    renderer: Mutex<StillRenderer>,
}

impl Session {
    /// The beautified image at the screenshot's native scale, as PNG.
    fn render_png(&self, background: &BackgroundSettings) -> Result<Vec<u8>, String> {
        let mut renderer = self.renderer.lock().map_err(|e| e.to_string())?;
        let (width, height) =
            recast_render::still::fit(renderer.native_size(background), MAX_SIDE, MAX_SIDE);
        let pixels = renderer
            .render(background, width, height)
            .map_err(|e| e.to_string())?;
        bitmap::encode_png(width, height, &pixels).map_err(|e| e.to_string())
    }
}

#[derive(Default)]
pub struct Beautifiers {
    windows: Mutex<HashMap<String, Arc<Session>>>,
    next_id: AtomicU64,
}

impl Beautifiers {
    fn session(&self, label: &str) -> Result<Arc<Session>, String> {
        self.windows
            .lock()
            .map_err(|e| e.to_string())?
            .get(label)
            .cloned()
            .ok_or_else(|| "this window has no screenshot".to_string())
    }
}

pub fn open_window(app: &AppHandle, capture: Arc<Capture>) -> Result<(), String> {
    let beautifiers = app.state::<Beautifiers>();
    let label = format!(
        "{LABEL_PREFIX}{}",
        beautifiers.next_id.fetch_add(1, Ordering::Relaxed)
    );
    let session = Arc::new(Session {
        renderer: Mutex::new(StillRenderer::new(
            capture.width,
            capture.height,
            capture.rgba.clone(),
        )),
        capture: capture.clone(),
    });
    beautifiers
        .windows
        .lock()
        .map_err(|e| e.to_string())?
        .insert(label.clone(), session);
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title(format!("Beautify — {}", capture.name))
        .inner_size(1120.0, 720.0)
        .min_inner_size(820.0, 520.0)
        .theme(Some(tauri::Theme::Dark))
        .build()
        .map_err(|e| e.to_string())?;
    // The thumbnail doesn't activate the app, so the new window has to.
    let _ = window.set_focus();
    let app = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            let removed = app
                .state::<Beautifiers>()
                .windows
                .lock()
                .ok()
                .and_then(|mut windows| windows.remove(&label));
            // Releasing the GPU device can take a moment; keep it off the main thread.
            std::thread::spawn(move || drop(removed));
        }
    });
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BeautifyInit {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    /// The background used last time.
    pub background: BackgroundSettings,
    pub saved_path: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn beautify_open(
    window: WebviewWindow,
    beautifiers: State<'_, Beautifiers>,
    settings: State<'_, SettingsStore>,
) -> Result<BeautifyInit, String> {
    let capture = beautifiers.session(window.label())?.capture.clone();
    Ok(BeautifyInit {
        name: capture.name.clone(),
        width: capture.width,
        height: capture.height,
        scale_factor: capture.scale_factor,
        background: usable(settings.get().beautify),
        saved_path: capture.saved_path().map(|p| p.display().to_string()),
    })
}

/// The beautified screenshot scaled to fit `max_width × max_height` pixels, as a PNG data URL.
#[tauri::command]
#[specta::specta]
pub async fn beautify_preview(
    window: WebviewWindow,
    beautifiers: State<'_, Beautifiers>,
    background: BackgroundSettings,
    max_width: u32,
    max_height: u32,
) -> Result<String, String> {
    let session = beautifiers.session(window.label())?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut renderer = session.renderer.lock().map_err(|e| e.to_string())?;
        let (width, height) = recast_render::still::fit(
            renderer.native_size(&background),
            max_width.clamp(16, MAX_SIDE),
            max_height.clamp(16, MAX_SIDE),
        );
        let pixels = renderer
            .render(&background, width, height)
            .map_err(|e| e.to_string())?;
        let png = bitmap::encode_png(width, height, &pixels).map_err(|e| e.to_string())?;
        Ok(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// `background`, with the default fill instead of an image that no longer exists.
fn usable(background: BackgroundSettings) -> BackgroundSettings {
    match &background.fill {
        BackgroundFill::Image { path } if !Path::new(path).is_file() => BackgroundSettings {
            fill: BackgroundSettings::default().fill,
            ..background
        },
        _ => background,
    }
}

fn remember(settings: &SettingsStore, background: &BackgroundSettings) {
    if let Err(e) = settings.update(|s| s.beautify = background.clone()) {
        log::warn!("{e}");
    }
}

#[tauri::command]
#[specta::specta]
pub async fn beautify_copy(
    window: WebviewWindow,
    beautifiers: State<'_, Beautifiers>,
    settings: State<'_, SettingsStore>,
    background: BackgroundSettings,
) -> Result<(), String> {
    let session = beautifiers.session(window.label())?;
    remember(&settings, &background);
    let png = tauri::async_runtime::spawn_blocking(move || session.render_png(&background))
        .await
        .map_err(|e| e.to_string())??;
    copy_png(&png)
}

/// Saves to `path`, or under a new name in `~/Pictures/Recast` when there is none.
/// Returns the saved file.
#[tauri::command]
#[specta::specta]
pub async fn beautify_save(
    window: WebviewWindow,
    beautifiers: State<'_, Beautifiers>,
    settings: State<'_, SettingsStore>,
    background: BackgroundSettings,
    path: Option<String>,
) -> Result<String, String> {
    let session = beautifiers.session(window.label())?;
    remember(&settings, &background);
    let name = format!("{} beautified", session.capture.name);
    let saved = tauri::async_runtime::spawn_blocking(move || {
        let png = session.render_png(&background)?;
        save(&png, path.as_deref().map(Path::new), &name)
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(saved.display().to_string())
}

fn save(png: &[u8], path: Option<&Path>, name: &str) -> Result<PathBuf, String> {
    match path {
        Some(path) => std::fs::write(path, png)
            .map(|_| path.to_path_buf())
            .map_err(|e| format!("cannot save {}: {e}", path.display())),
        None => {
            let dir = files::screenshots_dir();
            files::save_png(&dir, name, png)
                .map_err(|e| format!("cannot save to {}: {e}", dir.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_background_image_falls_back_to_the_default_fill() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("wallpaper.png");
        std::fs::write(&image, b"png").unwrap();
        let with_image = |path: &Path| BackgroundSettings {
            fill: BackgroundFill::Image {
                path: path.display().to_string(),
            },
            padding: 0.2,
            ..Default::default()
        };

        let kept = with_image(&image);
        assert_eq!(usable(kept.clone()), kept);

        let missing = usable(with_image(&dir.path().join("moved.png")));
        assert_eq!(missing.fill, BackgroundSettings::default().fill);
        assert_eq!(missing.padding, 0.2, "the rest is kept");
    }
}
