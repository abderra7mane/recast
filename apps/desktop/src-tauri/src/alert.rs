//! Native alerts for a menu bar app, which has no window to show errors in.

use objc2::MainThreadMarker;
use objc2_app_kit::NSApplication;
use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

/// Brings Recast to the front, so its alerts don't open behind other apps.
pub fn activate(app: &AppHandle) {
    let _ = app.run_on_main_thread(|| {
        let mtm = MainThreadMarker::new().expect("runs on the main thread");
        NSApplication::sharedApplication(mtm).activate();
    });
}

pub fn show(app: &AppHandle, kind: MessageDialogKind, title: &str, text: impl Into<String>) {
    activate(app);
    app.dialog()
        .message(text)
        .title(title)
        .kind(kind)
        .show(|_| {});
}

pub fn error(app: &AppHandle, text: impl Into<String>) {
    let text = text.into();
    log::warn!("{text}");
    show(app, MessageDialogKind::Error, "Recast", text);
}
