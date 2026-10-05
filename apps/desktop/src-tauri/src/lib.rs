pub mod recording;
#[cfg(feature = "synthetic")]
pub mod synthetic_capture;

use std::{path::Path, sync::Mutex};

use recast_capture::{DisplayInfo, ScreenCapture, WindowInfo};
use recast_input::{Permission, PermissionState, Permissions};
use recast_project::UnfinishedBundle;
use recording::{FinishedRecording, RecordingRequest, RecordingStatus, Session};
use tauri::State;
use tauri_specta::{Builder, collect_commands};

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
async fn stop_recording(state: State<'_, AppState>) -> Result<FinishedRecording, String> {
    let session = state
        .session
        .lock()
        .map_err(|e| e.to_string())?
        .take()
        .ok_or("no recording is running")?;
    session.stop()
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
    Builder::<tauri::Wry>::new().commands(collect_commands![
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
        .manage(AppState::default())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Recast");
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
