mod actions;
pub mod activation;
mod alert;
mod appkit;
mod crash;
pub mod diagnostics;
pub mod editor;
pub mod flow;
mod focus;
pub mod hotkeys;
pub mod logging;
mod login_item;
pub mod onboarding;
pub mod picker;
pub mod recording;
pub mod recording_ui;
pub mod screenshots;
pub mod settings;
pub mod shortcuts;
#[cfg(feature = "synthetic")]
pub mod synthetic_capture;
#[cfg(feature = "synthetic")]
pub mod synthetic_input;
pub mod tray;
mod updates;
pub mod windows;

use std::path::Path;

use editor::commands::{self as editor_commands, Editors, ExportFinished, ExportProgress};
use flow::{Flow, Phase};
use hotkeys::ShortcutStatus;
use recast_capture::picker::PickMode;
use recast_input::{Permission, PermissionState, Permissions};
use recast_project::UnfinishedBundle;
use recording::FinishedRecording;
use recording_ui::{RecorderUi, RecordingChanged};
use screenshots::markup::{self, Markups};
use serde::{Deserialize, Serialize};
use settings::{AppSettings, RecordingSettings, ScreenshotSettings, SettingsStore, UpdateSettings};
use shortcuts::ShortcutAction;
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_specta::{Builder, collect_commands, collect_events};

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

fn require_screen_recording(app: &AppHandle) -> Result<(), String> {
    if recast_input::check_permissions().screen_recording != PermissionState::Granted {
        windows::show(app, windows::ONBOARDING)?;
        return Err("Recast needs the Screen Recording permission.".into());
    }
    Ok(())
}

/// Picks a target with `mode` and starts recording it, after the countdown when that
/// is on.
#[tauri::command]
#[specta::specta]
async fn start_recording(app: AppHandle, mode: PickMode) -> Result<(), String> {
    require_screen_recording(&app)?;
    recording_ui::start(app, mode).await
}

/// Picks a target with `mode` and takes a screenshot of it; `None` when cancelled.
#[tauri::command]
#[specta::specta]
async fn take_screenshot(
    app: AppHandle,
    mode: PickMode,
) -> Result<Option<screenshots::ScreenshotTaken>, String> {
    require_screen_recording(&app)?;
    let taken = screenshots::take(&app, mode).await?;
    if let Some(taken) = &taken {
        for warning in &taken.warnings {
            log::warn!("screenshot: {warning}");
        }
    }
    Ok(taken)
}

#[tauri::command]
#[specta::specta]
async fn stop_recording(app: AppHandle) {
    recording_ui::stop(app).await;
}

#[tauri::command]
#[specta::specta]
async fn recording_phase(flow: State<'_, Flow>) -> Result<Phase, String> {
    Ok(flow.phase())
}

#[tauri::command]
#[specta::specta]
async fn list_unfinished(
    settings: State<'_, SettingsStore>,
) -> Result<Vec<UnfinishedBundle>, String> {
    recast_project::list_unfinished(&settings.get().recording.dir()).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
async fn recover_bundle(app: AppHandle, path: String) -> Result<FinishedRecording, String> {
    let project = recast_project::recover(Path::new(&path)).map_err(|e| e.to_string())?;
    tray::refresh(&app);
    Ok(FinishedRecording {
        bundle_path: path,
        project,
        warnings: Vec::new(),
    })
}

/// Moves a crashed recording's bundle to the Trash.
pub fn discard_bundle(path: &Path) -> Result<(), String> {
    let bundle = recast_project::Bundle::open_crashed(path).map_err(|e| e.to_string())?;
    screenshots::move_to_trash(bundle.path())
}

#[tauri::command]
#[specta::specta]
async fn discard_unfinished(path: String) -> Result<(), String> {
    discard_bundle(Path::new(&path))
}

#[tauri::command]
#[specta::specta]
async fn reveal_in_finder(path: String) {
    screenshots::reveal(Path::new(&path));
}

#[tauri::command]
#[specta::specta]
async fn open_recordings_folder(settings: State<'_, SettingsStore>) -> Result<(), String> {
    actions::open_folder(&settings.get().recording.dir());
    Ok(())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AppWindow {
    Library,
    Settings,
    Onboarding,
}

#[tauri::command]
#[specta::specta]
async fn open_window(app: AppHandle, window: AppWindow) -> Result<(), String> {
    windows::show(
        &app,
        match window {
            AppWindow::Library => windows::LIBRARY,
            AppWindow::Settings => windows::SETTINGS,
            AppWindow::Onboarding => windows::ONBOARDING,
        },
    )
}

#[tauri::command]
#[specta::specta]
async fn get_settings(settings: State<'_, SettingsStore>) -> Result<AppSettings, String> {
    Ok(settings.get())
}

#[tauri::command]
#[specta::specta]
async fn set_screenshot_settings(
    settings: State<'_, SettingsStore>,
    screenshots: ScreenshotSettings,
) -> Result<AppSettings, String> {
    settings.update(|s| s.screenshots = screenshots)
}

#[tauri::command]
#[specta::specta]
async fn set_recording_settings(
    app: AppHandle,
    settings: State<'_, SettingsStore>,
    recording: RecordingSettings,
) -> Result<AppSettings, String> {
    let saved = settings.update(|s| s.recording = recording)?;
    tray::refresh(&app);
    Ok(saved)
}

#[tauri::command]
#[specta::specta]
async fn set_update_settings(
    settings: State<'_, SettingsStore>,
    updates: UpdateSettings,
) -> Result<AppSettings, String> {
    settings.update(|s| s.updates = updates)
}

/// Changes an action's shortcut, `None` turning it off. A conflict is an error and
/// changes nothing; a shortcut macOS refuses is kept and reported in its status.
#[tauri::command]
#[specta::specta]
async fn set_shortcut(
    app: AppHandle,
    settings: State<'_, SettingsStore>,
    action: ShortcutAction,
    shortcut: Option<String>,
) -> Result<Vec<ShortcutStatus>, String> {
    let normalized = match &shortcut {
        Some(text) => Some(shortcuts::check(action, text, &settings.get().shortcuts)?.to_string()),
        None => None,
    };
    settings.update(|s| action.set(&mut s.shortcuts, normalized))?;
    let statuses = hotkeys::apply(&app);
    tray::refresh(&app);
    Ok(statuses)
}

#[tauri::command]
#[specta::specta]
async fn shortcut_statuses(app: AppHandle) -> Vec<ShortcutStatus> {
    hotkeys::statuses(&app)
}

/// Turns the global shortcuts off while the Settings window records a new one.
#[tauri::command]
#[specta::specta]
async fn suspend_shortcuts(app: AppHandle, suspended: bool) -> Vec<ShortcutStatus> {
    hotkeys::suspend(&app, suspended)
}

#[tauri::command]
#[specta::specta]
async fn check_for_updates(app: AppHandle) {
    updates::check(app, true).await;
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub logs_dir: String,
}

#[tauri::command]
#[specta::specta]
async fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        logs_dir: logging::config().dir.display().to_string(),
    }
}

#[tauri::command]
#[specta::specta]
async fn open_logs_folder() {
    actions::open_folder(&logging::config().dir);
}

#[tauri::command]
#[specta::specta]
async fn copy_diagnostics(app: AppHandle) -> Result<(), String> {
    let version = app.package_info().version.to_string();
    let text = tauri::async_runtime::spawn_blocking(move || {
        let info = diagnostics::SystemInfo::current(&version);
        let lines = logging::config().tail(diagnostics::LOG_LINES);
        let home = std::env::var("HOME").unwrap_or_default();
        diagnostics::text(&info, &lines, &home)
    })
    .await
    .map_err(|e| e.to_string())?;
    screenshots::copy_text(&text)
}

pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .events(collect_events![
            ExportProgress,
            ExportFinished,
            RecordingChanged
        ])
        .commands(collect_commands![
            check_permissions,
            request_permission,
            onboarding::open_privacy_settings,
            onboarding::relaunch,
            onboarding::complete_onboarding,
            start_recording,
            take_screenshot,
            stop_recording,
            recording_phase,
            list_unfinished,
            recover_bundle,
            discard_unfinished,
            reveal_in_finder,
            open_recordings_folder,
            open_window,
            get_settings,
            set_screenshot_settings,
            set_recording_settings,
            set_update_settings,
            set_shortcut,
            shortcut_statuses,
            suspend_shortcuts,
            login_item::get_launch_at_login,
            login_item::set_launch_at_login,
            login_item::open_login_items_settings,
            check_for_updates,
            app_info,
            open_logs_folder,
            copy_diagnostics,
            markup::markup_open,
            markup::markup_frame,
            markup::markup_finish,
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
            editor_commands::click_sound_preview,
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

fn setup(
    app: &mut tauri::App,
    last_run: Option<std::time::SystemTime>,
) -> Result<(), Box<dyn std::error::Error>> {
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    appkit::set_cursor_in_background();
    let settings_path = app.path().app_data_dir()?.join(settings::FILE_NAME);
    app.manage(SettingsStore::load(&settings_path));
    let handle = app.handle();
    log::info!("Recast {} started", app.package_info().version);
    crash::log_reports_since(last_run);

    for status in hotkeys::apply(handle) {
        if let Some(error) = status.error {
            log::warn!("shortcut for {:?}: {error}", status.action);
        }
    }
    tray::create(handle)?;

    for arg in std::env::args().skip(1) {
        let path = std::path::PathBuf::from(arg);
        if path
            .extension()
            .is_some_and(|e| e == recast_project::BUNDLE_EXTENSION)
            && let Err(e) = editor_commands::open_editor_window(handle, &path)
        {
            log::warn!("cannot open {}: {e}", path.display());
        }
    }

    // Development: opens the markup editor on a PNG without taking a screenshot.
    #[cfg(feature = "synthetic")]
    if let Some(path) = std::env::var_os("RECAST_MARKUP_PNG") {
        match screenshots::Capture::open(Path::new(&path), 2.0) {
            Ok(capture) => screenshots::edit(handle, std::sync::Arc::new(capture)),
            Err(e) => log::warn!("cannot open {}: {e}", Path::new(&path).display()),
        }
    }

    let settings = handle.state::<SettingsStore>().get();
    if !settings.onboarding_completed {
        windows::show_or_log(handle, windows::ONBOARDING);
    }
    let unfinished = recast_project::list_unfinished(&settings.recording.dir()).unwrap_or_default();
    if !unfinished.is_empty() {
        log::info!("{} unfinished recordings", unfinished.len());
        windows::show_or_log(handle, windows::LIBRARY);
    }
    if settings.updates.check_automatically {
        tauri::async_runtime::spawn(updates::check(handle.clone(), false));
    }
    Ok(())
}

/// The app's configuration and frontend assets, as compiled into the binary.
fn context() -> tauri::Context {
    tauri::generate_context!()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let last_run = crash::last_log_write(&logging::config().active_file());
    crash::install_handlers();
    let builder = specta_builder();
    tauri::Builder::default()
        .plugin(logging::config().plugin())
        .plugin(tauri_plugin_dialog::init())
        .plugin(hotkeys::plugin())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Editors::default())
        .manage(Markups::default())
        .manage(Flow::default())
        .manage(RecorderUi::default())
        .manage(activation::Tracker::default())
        .manage(hotkeys::Hotkeys::default())
        .manage(tray::TrayState::default())
        .invoke_handler(markup::with_raw_commands(builder.invoke_handler()))
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                activation::window_destroyed(window.app_handle(), window.label());
                if window.label() == windows::SETTINGS {
                    hotkeys::resume(window.app_handle());
                }
            }
        })
        .setup(move |app| {
            builder.mount_events(app);
            setup(app, last_run)
        })
        .build(context())
        .expect("error while building Recast")
        .run(|app, event| match event {
            // Closing the last window keeps Recast in the menu bar.
            tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } => api.prevent_exit(),
            tauri::RunEvent::Reopen {
                has_visible_windows: false,
                ..
            } => windows::show_or_log(app, windows::LIBRARY),
            // Quitting does not destroy windows first, so sessions are saved here.
            tauri::RunEvent::Exit => {
                recording_ui::save_on_quit(app);
                app.state::<Editors>().shutdown();
            }
            _ => {}
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

    /// Tauri's invoke fetches `ipc://localhost/<command>` (or `http://ipc.localhost`
    /// on some platforms). If the CSP blocks it, Tauri silently falls back to
    /// postMessage, which sends JSON and drops raw byte bodies.
    #[test]
    fn the_bundled_csp_lets_the_ipc_protocol_through() {
        let config = super::context().config().clone();
        let csp = config
            .app
            .security
            .csp
            .expect("a content security policy")
            .to_string();
        let connect = csp
            .split(';')
            .map(str::trim)
            .find_map(|directive| directive.strip_prefix("connect-src "))
            .unwrap_or_else(|| panic!("no connect-src in {csp}"));
        let sources: Vec<&str> = connect.split_whitespace().collect();
        for needed in ["'self'", "ipc:", "http://ipc.localhost", "ws://127.0.0.1:*"] {
            assert!(
                sources.contains(&needed),
                "connect-src lacks {needed}: {csp}"
            );
        }
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
