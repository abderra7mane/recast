pub mod editor;
pub mod recording;
#[cfg(feature = "synthetic")]
pub mod synthetic_capture;
#[cfg(feature = "synthetic")]
pub mod synthetic_input;

use std::{path::Path, sync::Mutex};

use editor::commands::{self as editor_commands, Editors, ExportFinished, ExportProgress};
use recast_capture::{DisplayInfo, ScreenCapture, WindowInfo};
use recast_input::{Permission, PermissionState, Permissions};
use recast_project::UnfinishedBundle;
use recording::{FinishedRecording, RecordingRequest, RecordingStatus, Session};
use tauri::{AppHandle, Manager, State};
use tauri_specta::{Builder, collect_commands, collect_events};

#[derive(Default)]
pub struct AppState {
    session: Mutex<Option<Session>>,
}

#[tauri::command]
#[specta::specta]
async fn list_displays() -> Result<Vec<DisplayInfo>, String> {
    recast_capture::platform()
        .displays()
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
async fn list_windows() -> Result<Vec<WindowInfo>, String> {
    recast_capture::platform()
        .windows()
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
async fn check_permissions() -> Permissions {
    recast_input::check_permissions()
}

#[tauri::command]
#[specta::specta]
async fn request_permission(permission: Permission) -> PermissionState {
    recast_input::request_permission(permission)
}

#[tauri::command]
#[specta::specta]
async fn start_recording(
    state: State<'_, AppState>,
    request: RecordingRequest,
) -> Result<RecordingStatus, String> {
    let mut slot = state.session.lock().map_err(|e| e.to_string())?;
    if slot.is_some() {
        return Err("a recording is already running".into());
    }
    let session = Session::start(
        &recast_project::default_root(),
        &recording::default_name(),
        &request,
        &recast_capture::platform(),
        &recast_input::platform(),
    )?;
    let status = session.status();
    *slot = Some(session);
    Ok(status)
}

#[tauri::command]
#[specta::specta]
async fn recording_status(state: State<'_, AppState>) -> Result<Option<RecordingStatus>, String> {
    let slot = state.session.lock().map_err(|e| e.to_string())?;
    Ok(slot.as_ref().map(Session::status))
}

#[tauri::command]
#[specta::specta]
async fn stop_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<FinishedRecording, String> {
    let session = state
        .session
        .lock()
        .map_err(|e| e.to_string())?
        .take()
        .ok_or("no recording is running")?;
    let finished = session.stop()?;
    if let Err(e) = editor_commands::open_editor_window(&app, Path::new(&finished.bundle_path)) {
        log::warn!("cannot open the editor: {e}");
    }
    Ok(finished)
}

#[tauri::command]
#[specta::specta]
async fn list_unfinished() -> Result<Vec<UnfinishedBundle>, String> {
    recast_project::list_unfinished(&recast_project::default_root()).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
async fn recover_bundle(path: String) -> Result<FinishedRecording, String> {
    let project = recast_project::recover(Path::new(&path)).map_err(|e| e.to_string())?;
    Ok(FinishedRecording {
        bundle_path: path,
        project,
        warnings: Vec::new(),
    })
}

/// Moves a crashed recording's bundle to the Trash.
pub fn discard_bundle(path: &Path) -> Result<(), String> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    let bundle = recast_project::Bundle::open_crashed(path).map_err(|e| e.to_string())?;
    let url = NSURL::fileURLWithPath(&NSString::from_str(&bundle.path().to_string_lossy()));
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|e| e.localizedDescription().to_string())
}

#[tauri::command]
#[specta::specta]
async fn discard_unfinished(path: String) -> Result<(), String> {
    discard_bundle(Path::new(&path))
}

#[tauri::command]
#[specta::specta]
async fn reveal_in_finder(path: String) -> Result<(), String> {
    std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(&path)
        .status()
        .map_err(|e| e.to_string())
        .map(|_| ())
}

pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .events(collect_events![ExportProgress, ExportFinished])
        .commands(collect_commands![
            list_displays,
            list_windows,
            check_permissions,
            request_permission,
            start_recording,
            recording_status,
            stop_recording,
            list_unfinished,
            recover_bundle,
            discard_unfinished,
            reveal_in_finder,
            editor_commands::list_projects,
            editor_commands::open_editor,
            editor_commands::editor_open,
            editor_commands::editor_set_settings,
            editor_commands::editor_play,
            editor_commands::editor_pause,
            editor_commands::editor_seek,
            editor_commands::editor_set_loop,
            editor_commands::editor_resize,
            editor_commands::editor_status,
            editor_commands::editor_stats,
            editor_commands::export_start,
            editor_commands::export_cancel,
        ])
}

pub const BINDINGS_PATH: &str = "src/bindings.ts";

pub fn export_bindings(path: &Path) -> Result<(), String> {
    specta_builder()
        .export(
            specta_typescript::Typescript::default().header("// @ts-nocheck\n"),
            path,
        )
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .manage(Editors::default())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            for arg in std::env::args().skip(1) {
                let path = std::path::PathBuf::from(arg);
                if path
                    .extension()
                    .is_some_and(|e| e == recast_project::BUNDLE_EXTENSION)
                    && let Err(e) = editor_commands::open_editor_window(app.handle(), &path)
                {
                    log::warn!("cannot open {}: {e}", path.display());
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Recast")
        .run(|app, event| {
            // Quitting does not destroy windows first, so sessions are saved here.
            if let tauri::RunEvent::Exit = event {
                app.state::<Editors>().shutdown();
            }
        });
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    #[test]
    fn discard_only_accepts_crashed_bundles() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("notes");
        std::fs::create_dir(&plain).unwrap();
        assert!(super::discard_bundle(&plain).is_err());
        assert!(plain.exists());

        let live = recast_project::Bundle::create(dir.path(), "Live").unwrap();
        assert!(super::discard_bundle(live.path()).is_err());
        assert!(live.path().exists());
    }

    #[test]
    fn bindings_are_up_to_date() {
        let committed = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(super::BINDINGS_PATH);
        let dir = tempfile::tempdir().unwrap();
        let fresh = dir.path().join("bindings.ts");
        super::export_bindings(&fresh).unwrap();
        assert!(
            std::fs::read_to_string(&committed).unwrap_or_default()
                == std::fs::read_to_string(&fresh).unwrap(),
            "run `make bindings` to regenerate {}",
            committed.display()
        );
    }
}
