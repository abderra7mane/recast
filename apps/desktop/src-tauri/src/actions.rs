//! What the menu bar items, the hotkeys and the control bar buttons do.

use std::path::Path;

use recast_capture::picker::PickMode;
use recast_input::PermissionState;
use tauri::{AppHandle, Manager};

use crate::{
    alert,
    flow::{Flow, Phase},
    recording_ui, screenshots,
    shortcuts::ShortcutAction,
    windows,
};

/// False, with onboarding shown, while Screen Recording is not granted.
fn can_capture(app: &AppHandle) -> bool {
    if recast_input::check_permissions().screen_recording == PermissionState::Granted {
        return true;
    }
    windows::show_or_log(app, windows::ONBOARDING);
    false
}

/// Runs a shortcut's or menu item's action. Any record action stops a running recording.
pub fn run(app: &AppHandle, action: ShortcutAction) {
    if action.records() {
        toggle_recording(app, action.mode());
    } else {
        screenshot(app, action.mode());
    }
}

pub fn toggle_recording(app: &AppHandle, mode: PickMode) {
    let idle = matches!(app.state::<Flow>().phase(), Phase::Idle);
    if idle && !can_capture(app) {
        return;
    }
    tauri::async_runtime::spawn(recording_ui::toggle(app.clone(), mode));
}

pub fn stop_recording(app: &AppHandle) {
    tauri::async_runtime::spawn(recording_ui::stop(app.clone()));
}

pub fn restart_recording(app: &AppHandle) {
    tauri::async_runtime::spawn(recording_ui::restart(app.clone()));
}

pub fn cancel_recording(app: &AppHandle) {
    tauri::async_runtime::spawn(recording_ui::cancel(app.clone()));
}

pub fn screenshot(app: &AppHandle, mode: PickMode) {
    if !can_capture(app) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match screenshots::take(&app, mode).await {
            Ok(Some(taken)) => {
                for warning in taken.warnings {
                    log::warn!("screenshot: {warning}");
                }
            }
            Ok(None) => {}
            Err(e) => alert::error(&app, format!("The screenshot failed: {e}")),
        }
    });
}

/// Opens `dir` in Finder, creating it first.
pub fn open_folder(dir: &Path) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        log::warn!("cannot create {}: {e}", dir.display());
    }
    if let Err(e) = std::process::Command::new("/usr/bin/open")
        .arg(dir)
        .status()
    {
        log::warn!("cannot open {}: {e}", dir.display());
    }
}
