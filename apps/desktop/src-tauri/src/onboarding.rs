//! Permissions setup: deep links into System Settings and relaunching after Screen
//! Recording is granted, which macOS only applies to a new process.

use std::path::{Path, PathBuf};

use recast_input::Permission;
use tauri::{AppHandle, State, WebviewWindow};

use crate::settings::{AppSettings, SettingsStore};

/// The Privacy & Security pane in System Settings that lists `permission`.
pub fn privacy_url(permission: Permission) -> &'static str {
    match permission {
        Permission::ScreenRecording => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        }
        Permission::InputMonitoring => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
        }
        Permission::Microphone => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
    }
}

/// `Recast.app` for an executable at `Recast.app/Contents/MacOS/<name>`.
pub fn app_bundle(executable: &Path) -> Option<PathBuf> {
    let macos = executable.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    (macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app")
        .then(|| bundle.to_path_buf())
}

#[tauri::command]
#[specta::specta]
pub async fn open_privacy_settings(permission: Permission) -> Result<(), String> {
    std::process::Command::new("/usr/bin/open")
        .arg(privacy_url(permission))
        .status()
        .map_err(|e| e.to_string())
        .map(|_| ())
}

/// Quits and opens Recast again through Launch Services.
#[tauri::command]
#[specta::specta]
pub async fn relaunch(app: AppHandle) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let Some(bundle) = app_bundle(&executable) else {
        app.request_restart();
        return Ok(());
    };
    let script = format!(
        "while /bin/kill -0 {} 2>/dev/null; do /bin/sleep 0.2; done; /usr/bin/open \"$0\"",
        std::process::id()
    );
    std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .arg(&bundle)
        .spawn()
        .map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

/// Remembers that onboarding is done and closes its window.
#[tauri::command]
#[specta::specta]
pub async fn complete_onboarding(
    window: WebviewWindow,
    settings: State<'_, SettingsStore>,
) -> Result<AppSettings, String> {
    let saved = settings.update(|s| s.onboarding_completed = true)?;
    if window.label() == crate::windows::ONBOARDING {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_permission_opens_its_privacy_pane() {
        assert!(privacy_url(Permission::ScreenRecording).ends_with("?Privacy_ScreenCapture"));
        assert!(privacy_url(Permission::InputMonitoring).ends_with("?Privacy_ListenEvent"));
        assert!(privacy_url(Permission::Microphone).ends_with("?Privacy_Microphone"));
    }

    #[test]
    fn finds_the_app_bundle_of_an_executable() {
        assert_eq!(
            app_bundle(Path::new(
                "/Applications/Recast.app/Contents/MacOS/recast-desktop"
            )),
            Some(PathBuf::from("/Applications/Recast.app"))
        );
        assert_eq!(
            app_bundle(Path::new("/repo/target/debug/recast-desktop")),
            None
        );
        assert_eq!(app_bundle(Path::new("/x/Contents/MacOS/recast")), None);
    }
}
