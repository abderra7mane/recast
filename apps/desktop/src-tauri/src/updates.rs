//! Update checks against the `latest.json` of the releases repo. The Update window
//! shows the result and offers to install and relaunch.

use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_updater::{Update, UpdaterExt};
use tauri_specta::Event;

/// Marks the endpoint and key in `tauri.conf.json` that a release build replaces.
const PLACEHOLDERS: &[&str] = &["<owner>", "REPLACE_WITH_UPDATER_PUBLIC_KEY"];

static CHECKING: AtomicBool = AtomicBool::new(false);
static AVAILABLE: AtomicBool = AtomicBool::new(false);
static INSTALLING: AtomicBool = AtomicBool::new(false);
/// A manual check asked for while another check runs, which reports its result.
static REPORT: AtomicBool = AtomicBool::new(false);

/// The update the last check found, and what the Update window shows.
#[derive(Default)]
pub struct Pending {
    update: Mutex<Option<Update>>,
    status: Mutex<UpdateStatus>,
}

impl Pending {
    fn set(&self, update: Option<Update>) {
        *self.update.lock().unwrap_or_else(|e| e.into_inner()) = update;
    }

    fn get(&self) -> Option<Update> {
        self.update
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn status(&self) -> UpdateStatus {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn set_status(&self, status: UpdateStatus) {
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = status;
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UpdateStatus {
    #[default]
    Checking,
    Available(UpdateInfo),
    UpToDate {
        version: String,
    },
    NotConfigured,
    Failed {
        error: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct UpdateStatusChanged {
    pub status: UpdateStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
}

/// Bytes of the update downloaded so far; `total` is unknown when the server
/// doesn't send a length.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub downloaded: u32,
    pub total: Option<u32>,
}

/// Whether the last check found an update that is not installed yet.
pub fn available() -> bool {
    AVAILABLE.load(Ordering::Relaxed)
}

fn set_available(app: &AppHandle, available: bool) {
    if AVAILABLE.swap(available, Ordering::Relaxed) != available {
        crate::tray::refresh(app);
    }
}

/// Whether the updater config has a real endpoint and public key.
pub fn configured(updater: Option<&serde_json::Value>) -> bool {
    let Some(updater) = updater else {
        return false;
    };
    let pubkey = updater["pubkey"].as_str().unwrap_or_default();
    let endpoints: Vec<&str> = updater["endpoints"]
        .as_array()
        .map(|list| list.iter().filter_map(|e| e.as_str()).collect())
        .unwrap_or_default();
    !pubkey.trim().is_empty()
        && !endpoints.is_empty()
        && !PLACEHOLDERS
            .iter()
            .any(|p| pubkey.contains(p) || endpoints.iter().any(|e| e.contains(p)))
}

enum Outcome {
    NotConfigured,
    UpToDate,
    Available(Box<Update>),
}

/// Checks for an update and shows it in the Update window. A manual check also
/// shows "up to date" and errors there; an automatic one only logs them.
pub async fn check(app: AppHandle, manual: bool) {
    if manual {
        REPORT.store(true, Ordering::SeqCst);
        report(&app, UpdateStatus::Checking);
    }
    if CHECKING.swap(true, Ordering::SeqCst) {
        return;
    }
    let result = run(&app).await;
    CHECKING.store(false, Ordering::SeqCst);
    let manual = REPORT.swap(false, Ordering::SeqCst);
    let status = match result {
        Err(e) => {
            log::warn!("update check failed: {e}");
            UpdateStatus::Failed { error: e }
        }
        Ok(Outcome::NotConfigured) => {
            log::info!("updates are not set up for this build");
            UpdateStatus::NotConfigured
        }
        Ok(Outcome::UpToDate) => {
            log::info!("Recast is up to date");
            set_available(&app, false);
            app.state::<Pending>().set(None);
            UpdateStatus::UpToDate {
                version: app.package_info().version.to_string(),
            }
        }
        Ok(Outcome::Available(update)) => {
            log::info!("update {} is available", update.version);
            set_available(&app, true);
            let info = info(&update);
            app.state::<Pending>().set(Some(*update));
            report(&app, UpdateStatus::Available(info));
            return;
        }
    };
    if manual {
        report(&app, status);
    }
}

async fn run(app: &AppHandle) -> Result<Outcome, String> {
    if !configured(app.config().plugins.0.get("updater")) {
        return Ok(Outcome::NotConfigured);
    }
    let updater = app.updater().map_err(|e| e.to_string())?;
    Ok(match updater.check().await.map_err(|e| e.to_string())? {
        Some(update) => Outcome::Available(Box::new(update)),
        None => Outcome::UpToDate,
    })
}

fn info(update: &Update) -> UpdateInfo {
    let title = format!("Recast {}", update.version);
    UpdateInfo {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        notes: update
            .body
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty() && *n != title)
            .map(str::to_owned),
    }
}

fn report(app: &AppHandle, status: UpdateStatus) {
    app.state::<Pending>().set_status(status.clone());
    let _ = UpdateStatusChanged { status }.emit(app);
    crate::windows::show_or_log(app, crate::windows::UPDATE);
}

#[tauri::command]
#[specta::specta]
pub async fn update_status(pending: tauri::State<'_, Pending>) -> Result<UpdateStatus, String> {
    Ok(pending.status())
}

/// Downloads and installs the pending update, then relaunches Recast. Progress
/// arrives as `UpdateProgress` events.
#[tauri::command]
#[specta::specta]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let update = app
        .state::<Pending>()
        .get()
        .ok_or("There is no update to install.")?;
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let mut downloaded = 0u32;
    let result = update
        .download_and_install(
            |chunk, total| {
                downloaded = downloaded.saturating_add(chunk as u32);
                let _ = UpdateProgress {
                    downloaded,
                    total: total.map(|t| t as u32),
                }
                .emit(&app);
            },
            || log::info!("update downloaded"),
        )
        .await;
    INSTALLING.store(false, Ordering::SeqCst);
    result.map_err(|e| {
        log::warn!("update install failed: {e}");
        e.to_string()
    })?;
    log::info!("update installed, relaunching");
    app.state::<crate::editor::commands::Editors>().shutdown();
    app.restart();
}

#[tauri::command]
#[specta::specta]
pub async fn dismiss_update(window: WebviewWindow) -> Result<(), String> {
    if INSTALLING.load(Ordering::SeqCst) || window.label() != crate::windows::UPDATE {
        return Ok(());
    }
    window.close().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn placeholders_mean_not_configured() {
        let shipped: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert!(!configured(shipped["plugins"].get("updater")));
        assert!(!configured(None));
        assert!(!configured(Some(&json!({
            "endpoints": ["https://github.com/ada/recast-releases/releases/latest/download/latest.json"],
            "pubkey": ""
        }))));
        assert!(!configured(Some(
            &json!({ "endpoints": [], "pubkey": "abc" })
        )));
    }

    #[test]
    fn the_shipped_placeholders_still_load() {
        let shipped: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let config: tauri_plugin_updater::Config =
            serde_json::from_value(shipped["plugins"]["updater"].clone()).unwrap();
        assert_eq!(config.endpoints.len(), 1);
        assert_eq!(config.endpoints[0].scheme(), "https");
        assert!(
            config.require_signed_version,
            "no downgrades under a newer version"
        );
    }

    #[test]
    fn a_real_endpoint_and_key_are_configured() {
        assert!(configured(Some(&json!({
            "endpoints": ["https://github.com/ada/recast-releases/releases/latest/download/latest.json"],
            "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk="
        }))));
    }
}
