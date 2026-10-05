//! App preferences, kept as JSON in the app's support directory.

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use recast_project::BackgroundSettings;
use serde::{Deserialize, Serialize};
use specta::Type;

pub const FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ScreenshotSettings {
    pub copy_to_clipboard: bool,
    pub save_to_disk: bool,
    /// Where captures are saved; `None` means `~/Pictures/Recast`.
    pub folder: Option<String>,
}

impl Default for ScreenshotSettings {
    fn default() -> Self {
        Self {
            copy_to_clipboard: true,
            save_to_disk: true,
            folder: None,
        }
    }
}

impl ScreenshotSettings {
    pub fn dir(&self) -> PathBuf {
        folder_or(&self.folder, crate::screenshots::files::screenshots_dir)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct RecordingSettings {
    /// Counts down from 3 before recording starts.
    pub countdown: bool,
    pub mic: bool,
    pub system_audio: bool,
    /// Where recordings are saved; `None` means `~/Movies/Recast`.
    pub folder: Option<String>,
}

impl Default for RecordingSettings {
    fn default() -> Self {
        Self {
            countdown: true,
            mic: false,
            system_audio: true,
            folder: None,
        }
    }
}

impl RecordingSettings {
    pub fn dir(&self) -> PathBuf {
        folder_or(&self.folder, recast_project::default_root)
    }
}

fn folder_or(folder: &Option<String>, default: impl FnOnce() -> PathBuf) -> PathBuf {
    folder
        .as_deref()
        .filter(|f| !f.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default)
}

/// Global shortcuts in the form `Alt+Shift+Cmd+KeyR`; `None` turns one off.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ShortcutSettings {
    pub record: Option<String>,
    pub capture_area: Option<String>,
    pub capture_window: Option<String>,
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            record: Some("Alt+Shift+Cmd+KeyR".into()),
            capture_area: Some("Alt+Shift+Cmd+KeyS".into()),
            capture_window: Some("Alt+Shift+Cmd+KeyW".into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateSettings {
    pub check_automatically: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            check_automatically: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub screenshots: ScreenshotSettings,
    /// The last background used to beautify a screenshot.
    pub beautify: BackgroundSettings,
    pub recording: RecordingSettings,
    pub shortcuts: ShortcutSettings,
    pub updates: UpdateSettings,
    pub onboarding_completed: bool,
}

pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<AppSettings>,
}

impl SettingsStore {
    /// Reads the settings at `path`; a missing or unreadable file gives the defaults.
    pub fn load(path: &Path) -> Self {
        let current = match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                log::warn!("ignoring unreadable {}: {e}", path.display());
                AppSettings::default()
            }),
            Err(_) => AppSettings::default(),
        };
        Self {
            path: path.to_path_buf(),
            current: Mutex::new(current),
        }
    }

    pub fn get(&self) -> AppSettings {
        self.current.lock().expect("settings lock").clone()
    }

    /// Changes the settings and writes them to disk.
    pub fn update(&self, change: impl FnOnce(&mut AppSettings)) -> Result<AppSettings, String> {
        let mut current = self.current.lock().map_err(|e| e.to_string())?;
        let mut next = current.clone();
        change(&mut next);
        if next != *current {
            write_atomically(
                &self.path,
                &serde_json::to_vec_pretty(&next).map_err(|e| e.to_string())?,
            )
            .map_err(|e| format!("cannot save {}: {e}", self.path.display()))?;
            *current = next;
        }
        Ok(current.clone())
    }
}

fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path)
}

#[cfg(test)]
mod tests {
    use recast_project::{BackgroundFill, Color};

    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(&dir.path().join(FILE_NAME));
        let settings = store.get();
        assert!(settings.screenshots.copy_to_clipboard);
        assert!(settings.screenshots.save_to_disk);
        assert_eq!(settings.beautify, BackgroundSettings::default());
    }

    #[test]
    fn changes_persist_across_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join(FILE_NAME);
        let store = SettingsStore::load(&path);
        store
            .update(|s| {
                s.screenshots.save_to_disk = false;
                s.beautify.fill = BackgroundFill::Solid {
                    color: Color::rgb(1, 2, 3),
                };
            })
            .unwrap();

        let reloaded = SettingsStore::load(&path).get();
        assert!(!reloaded.screenshots.save_to_disk);
        assert!(reloaded.screenshots.copy_to_clipboard);
        assert_eq!(
            reloaded.beautify.fill,
            BackgroundFill::Solid {
                color: Color::rgb(1, 2, 3)
            }
        );
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn partial_and_corrupt_files_fall_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(
            &path,
            r#"{"screenshots":{"copyToClipboard":false},"future":1}"#,
        )
        .unwrap();
        let settings = SettingsStore::load(&path).get();
        assert!(!settings.screenshots.copy_to_clipboard);
        assert!(settings.screenshots.save_to_disk);

        std::fs::write(&path, b"{not json").unwrap();
        assert_eq!(SettingsStore::load(&path).get(), AppSettings::default());
    }

    #[test]
    fn m4_settings_keep_their_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(
            &path,
            r##"{
  "screenshots": { "copyToClipboard": false, "saveToDisk": true },
  "beautify": {
    "fill": { "kind": "solid", "color": "#0a141e" },
    "padding": 0.2,
    "cornerRadius": 0.03,
    "shadow": { "opacity": 0.3, "blur": 0.05, "offsetY": 0.01 }
  }
}"##,
        )
        .unwrap();
        let settings = SettingsStore::load(&path).get();
        assert!(!settings.screenshots.copy_to_clipboard);
        assert!(settings.screenshots.save_to_disk);
        assert_eq!(settings.screenshots.folder, None);
        assert_eq!(
            settings.beautify.fill,
            BackgroundFill::Solid {
                color: Color::rgb(10, 20, 30)
            }
        );
        assert_eq!(settings.beautify.padding, 0.2);
        assert_eq!(settings.beautify.shadow.opacity, 0.3);
        assert_eq!(settings.recording, RecordingSettings::default());
        assert_eq!(settings.shortcuts, ShortcutSettings::default());
        assert!(settings.updates.check_automatically);
        assert!(!settings.onboarding_completed);

        let store = SettingsStore::load(&path);
        store.update(|s| s.onboarding_completed = true).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["screenshots"]["copyToClipboard"], false);
        assert_eq!(saved["beautify"]["padding"], 0.2);
        assert_eq!(saved["shortcuts"]["record"], "Alt+Shift+Cmd+KeyR");
    }

    #[test]
    fn a_turned_off_shortcut_stays_off() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(&path, r#"{"shortcuts":{"captureArea":null}}"#).unwrap();
        let shortcuts = SettingsStore::load(&path).get().shortcuts;
        assert_eq!(shortcuts.capture_area, None);
        assert_eq!(shortcuts.record, ShortcutSettings::default().record);
    }

    #[test]
    fn folders_default_to_movies_and_pictures() {
        let settings = AppSettings::default();
        assert!(settings.recording.dir().ends_with("Movies/Recast"));
        assert!(settings.screenshots.dir().ends_with("Pictures/Recast"));
        let custom = RecordingSettings {
            folder: Some("/tmp/takes".into()),
            ..Default::default()
        };
        assert_eq!(custom.dir(), PathBuf::from("/tmp/takes"));
        let blank = RecordingSettings {
            folder: Some(" ".into()),
            ..Default::default()
        };
        assert!(blank.dir().ends_with("Movies/Recast"));
    }

    #[test]
    fn unchanged_settings_are_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        SettingsStore::load(&path).update(|_| {}).unwrap();
        assert!(!path.exists());
    }
}
