//! Launch at login through `SMAppService`, which lists Recast under Login Items in
//! System Settings and follows the app when it moves.

use objc2_service_management::{SMAppService, SMAppServiceStatus};
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum LoginItem {
    Enabled,
    Disabled,
    /// The user has to allow it in System Settings → General → Login Items.
    NeedsApproval,
}

fn status() -> LoginItem {
    // SAFETY: `mainAppService` and `status` have no preconditions.
    let status = unsafe { SMAppService::mainAppService().status() };
    match status {
        SMAppServiceStatus::Enabled => LoginItem::Enabled,
        SMAppServiceStatus::RequiresApproval => LoginItem::NeedsApproval,
        _ => LoginItem::Disabled,
    }
}

#[tauri::command]
#[specta::specta]
pub async fn get_launch_at_login() -> LoginItem {
    status()
}

#[tauri::command]
#[specta::specta]
pub async fn set_launch_at_login(enabled: bool) -> Result<LoginItem, String> {
    // SAFETY: as in `status`; registering only works from the app bundle, which
    // the returned error reports.
    let result = unsafe {
        let service = SMAppService::mainAppService();
        if enabled {
            service.registerAndReturnError()
        } else {
            service.unregisterAndReturnError()
        }
    };
    match result {
        Ok(()) => Ok(status()),
        Err(e) if !enabled && status() == LoginItem::Disabled => {
            log::info!(
                "launch at login was already off: {}",
                e.localizedDescription()
            );
            Ok(LoginItem::Disabled)
        }
        Err(e) => Err(e.localizedDescription().to_string()),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn open_login_items_settings() {
    // SAFETY: no preconditions.
    unsafe { SMAppService::openSystemSettingsLoginItems() };
}
