//! Update checks against the `latest.json` of the releases repo, with an
//! "Install and Relaunch" prompt.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;

/// Marks the endpoint and key in `tauri.conf.json` that a release build replaces.
const PLACEHOLDERS: &[&str] = &["<owner>", "REPLACE_WITH_UPDATER_PUBLIC_KEY"];

static CHECKING: AtomicBool = AtomicBool::new(false);

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

fn tell(app: &AppHandle, kind: MessageDialogKind, title: &str, text: String) {
    crate::alert::show(app, kind, title, text);
}

/// Checks for an update and offers to install it. A manual check also reports
/// "up to date" and errors; an automatic one only logs them.
pub async fn check(app: AppHandle, manual: bool) {
    if CHECKING.swap(true, Ordering::SeqCst) {
        return;
    }
    let result = run(&app, manual).await;
    CHECKING.store(false, Ordering::SeqCst);
    if let Err(e) = result {
        log::warn!("update check failed: {e}");
        if manual {
            tell(
                &app,
                MessageDialogKind::Error,
                "Recast",
                format!("Couldn't check for updates.\n\n{e}"),
            );
        }
    }
}

async fn run(app: &AppHandle, manual: bool) -> Result<(), String> {
    if !configured(app.config().plugins.0.get("updater")) {
        log::info!("updates are not set up for this build");
        if manual {
            tell(
                app,
                MessageDialogKind::Info,
                "Recast",
                "Updates aren't set up for this build of Recast.".into(),
            );
        }
        return Ok(());
    }
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        log::info!("Recast is up to date");
        if manual {
            tell(
                app,
                MessageDialogKind::Info,
                "You're up to date",
                format!("Recast {} is the newest version.", update_version(app)),
            );
        }
        return Ok(());
    };
    log::info!("update {} is available", update.version);
    let mut text = format!(
        "Recast {} is available. You have {}.",
        update.version, update.current_version
    );
    if let Some(notes) = update.body.as_deref().filter(|n| !n.trim().is_empty()) {
        text.push_str("\n\n");
        text.push_str(notes.trim());
    }
    crate::alert::activate(app);
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .message(text)
        .title("Update available")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Install and Relaunch".into(),
            "Later".into(),
        ))
        .show(move |install| {
            let _ = tx.send(install);
        });
    if !rx.await.unwrap_or(false) {
        return Ok(());
    }
    update
        .download_and_install(|_, _| {}, || log::info!("update downloaded"))
        .await
        .map_err(|e| e.to_string())?;
    log::info!("update installed, relaunching");
    app.state::<crate::editor::commands::Editors>().shutdown();
    app.restart();
}

fn update_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
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
