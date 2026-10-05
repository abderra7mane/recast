//! The target picker shared by screenshots and recording.

pub mod cursor;
mod overlay;

use std::sync::atomic::{AtomicBool, Ordering};

use objc2::MainThreadMarker;
use recast_capture::{
    ScreenCapture,
    picker::{PickMode, Picked},
};
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::focus::Focus;

static OPEN: AtomicBool = AtomicBool::new(false);

struct OpenGuard;

impl Drop for OpenGuard {
    fn drop(&mut self) {
        OPEN.store(false, Ordering::SeqCst);
    }
}

pub struct Pick {
    pub picked: Picked,
    /// Activation taken from the app the user was in; give it back once the overlays
    /// that follow the pick are gone.
    pub focus: Focus,
}

/// Lets the user pick a display, window or region; `None` when they cancel.
pub async fn pick(app: &AppHandle, mode: PickMode) -> Result<Option<Pick>, String> {
    if OPEN.swap(true, Ordering::SeqCst) {
        return Err("The picker is already open.".into());
    }
    let _guard = OpenGuard;
    let windows =
        tauri::async_runtime::spawn_blocking(|| recast_capture::platform().window_stack())
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let own_pid = std::process::id() as i32;
    app.run_on_main_thread(move || {
        let mtm = MainThreadMarker::new().expect("runs on the main thread");
        let focus = Focus::take(mtm);
        overlay::open(
            mtm,
            mode,
            windows,
            own_pid,
            Box::new(move |picked| {
                let pick = match picked {
                    Some(picked) => Some(Pick { picked, focus }),
                    None => {
                        focus.restore(mtm);
                        None
                    }
                };
                let _ = tx.send(pick);
            }),
        );
    })
    .map_err(|e| e.to_string())?;
    rx.await
        .map_err(|_| "The picker closed unexpectedly.".to_string())
}

/// Hides the Library window so it doesn't cover what the user is picking.
pub fn hide_library(app: &AppHandle) -> Option<WebviewWindow> {
    let library = app
        .get_webview_window(crate::windows::LIBRARY)
        .filter(|w| w.is_visible().unwrap_or(false))?;
    let _ = library.hide();
    Some(library)
}

pub fn show_library(library: Option<WebviewWindow>) {
    if let Some(library) = library {
        let _ = library.show();
    }
}
