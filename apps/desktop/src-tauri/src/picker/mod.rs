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

static OPEN: AtomicBool = AtomicBool::new(false);

struct OpenGuard;

impl Drop for OpenGuard {
    fn drop(&mut self) {
        OPEN.store(false, Ordering::SeqCst);
    }
}

/// Lets the user pick a display, window or region; `None` when they cancel.
pub async fn pick(app: &AppHandle, mode: PickMode) -> Result<Option<Picked>, String> {
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
        overlay::open(
            mtm,
            mode,
            windows,
            own_pid,
            Box::new(move |picked| {
                let _ = tx.send(picked);
            }),
        );
    })
    .map_err(|e| e.to_string())?;
    rx.await
        .map_err(|_| "The picker closed unexpectedly.".to_string())
}

/// Hides the main window so it doesn't cover what the user is picking.
pub fn hide_main(app: &AppHandle) -> Option<WebviewWindow> {
    let main = app
        .get_webview_window("main")
        .filter(|w| w.is_visible().unwrap_or(false))?;
    let _ = main.hide();
    Some(main)
}

pub fn show_main(main: Option<WebviewWindow>) {
    if let Some(main) = main {
        let _ = main.show();
    }
}

#[tauri::command]
#[specta::specta]
pub async fn pick_target(app: AppHandle, mode: PickMode) -> Result<Option<Picked>, String> {
    let main = hide_main(&app);
    let picked = pick(&app, mode).await;
    show_main(main);
    picked
}
