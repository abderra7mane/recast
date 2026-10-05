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
    /// Saves every capture to `~/Pictures/Recast`.
    pub save_to_disk: bool,
}

impl Default for ScreenshotSettings {
    fn default() -> Self {
        Self {
            copy_to_clipboard: true,
            save_to_disk: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub screenshots: ScreenshotSettings,
    /// The last background used to beautify a screenshot.
    pub beautify: BackgroundSettings,
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
    fn unchanged_settings_are_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        SettingsStore::load(&path).update(|_| {}).unwrap();
        assert!(!path.exists());
    }
}
