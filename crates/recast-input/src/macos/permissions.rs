use cidre::{av, cg};

use super::block_on;
use crate::{Permission, PermissionState, Permissions};

fn flag(granted: bool) -> PermissionState {
    if granted {
        PermissionState::Granted
    } else {
        PermissionState::Denied
    }
}

fn microphone() -> PermissionState {
    match av::CaptureDevice::authorization_status_for_media_type(av::MediaType::audio()) {
        Ok(av::AuthorizationStatus::Authorized) => PermissionState::Granted,
        Ok(av::AuthorizationStatus::NotDetermined) => PermissionState::NotDetermined,
        _ => PermissionState::Denied,
    }
}

/// Screen Recording and Input Monitoring only report granted or not; macOS gives no
/// "not asked yet" state for them.
pub fn check_permissions() -> Permissions {
    Permissions {
        screen_recording: flag(cg::screen_capture_access::preflight()),
        input_monitoring: flag(cg::event::access::listen_preflight()),
        microphone: microphone(),
    }
}

/// Shows the system prompt when macOS still allows it, then returns the new state.
/// Once denied, the user has to change it in System Settings.
pub fn request_permission(permission: Permission) -> PermissionState {
    match permission {
        Permission::ScreenRecording => flag(cg::screen_capture_access::request()),
        Permission::InputMonitoring => flag(cg::event::access::listen_request()),
        Permission::Microphone => {
            if microphone() == PermissionState::NotDetermined {
                let _ = block_on(av::CaptureDevice::request_access_for_media_type(
                    av::MediaType::audio(),
                ));
            }
            microphone()
        }
    }
}
