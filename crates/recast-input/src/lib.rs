use std::{path::PathBuf, sync::Arc};

use recast_project::{CursorShape, EventKind};
use serde::{Deserialize, Serialize};
use specta::Type;

#[cfg(target_os = "macos")]
pub mod macos;

#[derive(Debug, Clone, PartialEq)]
pub enum InputRecord {
    /// `host_ns` is on the host clock, the clock ScreenCaptureKit stamps frames with.
    Event { host_ns: u64, kind: EventKind },
    /// A cursor shape seen for the first time; its PNG is already written.
    Shape(CursorShape),
}

pub type InputSink = Arc<dyn Fn(InputRecord) + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("input monitoring permission is missing")]
    PermissionDenied,
    #[error("{0}")]
    Platform(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub trait InputCapture: Send + Sync {
    /// Starts listening to mouse input and sampling the cursor shape. Cursor images
    /// are written to `cursors_dir`; `relative_dir` is how shapes refer to that folder.
    fn start(
        &self,
        cursors_dir: PathBuf,
        relative_dir: String,
        sink: InputSink,
    ) -> Result<Box<dyn ActiveInput>>;
}

pub trait ActiveInput: Send {
    /// Whether mouse events are recorded (false when Input Monitoring is missing).
    fn listening(&self) -> bool;
    fn stop(self: Box<Self>);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    ScreenRecording,
    InputMonitoring,
    Microphone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PermissionState {
    Granted,
    Denied,
    NotDetermined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Permissions {
    pub screen_recording: PermissionState,
    pub input_monitoring: PermissionState,
    pub microphone: PermissionState,
}

#[cfg(target_os = "macos")]
pub fn platform() -> impl InputCapture {
    macos::MacInput
}

#[cfg(target_os = "macos")]
pub use macos::permissions::{check_permissions, request_permission};

/// CGEvent timestamps are documented as nanoseconds but are mach ticks on Apple
/// Silicon. Picks the reading closest to `now_ns` and returns nanoseconds.
pub fn event_time_ns(raw: u64, now_ns: u64, numer: u32, denom: u32) -> u64 {
    let from_ticks = (raw as u128 * numer as u128 / denom.max(1) as u128) as u64;
    if now_ns.abs_diff(from_ticks) <= now_ns.abs_diff(raw) {
        from_ticks
    } else {
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_time_handles_ticks_and_nanoseconds() {
        let (numer, denom) = (125, 3);
        let now_ticks = 90_000_000_000u64;
        let now_ns = now_ticks * 125 / 3;
        let tick_event = now_ticks - 240;
        assert_eq!(
            event_time_ns(tick_event, now_ns, numer, denom),
            tick_event * 125 / 3
        );
        let ns_event = now_ns - 10_000;
        assert_eq!(event_time_ns(ns_event, now_ns, numer, denom), ns_event);
        assert_eq!(event_time_ns(5_000, 5_100, 1, 1), 5_000);
    }

    #[test]
    fn permission_json_shape() {
        let json = serde_json::to_value(Permissions {
            screen_recording: PermissionState::Granted,
            input_monitoring: PermissionState::Denied,
            microphone: PermissionState::NotDetermined,
        })
        .unwrap();
        assert_eq!(json["screenRecording"], "granted");
        assert_eq!(json["microphone"], "notDetermined");
    }
}
