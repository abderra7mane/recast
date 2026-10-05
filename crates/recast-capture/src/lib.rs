use std::{path::PathBuf, sync::Arc};

use serde::{Deserialize, Serialize};
use specta::Type;

#[cfg(target_os = "macos")]
pub mod macos;

pub use recast_project::Rect;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub id: u32,
    pub name: String,
    /// Global display points.
    pub bounds: Rect,
    pub scale_factor: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    pub app_name: String,
    /// Global display points.
    pub bounds: Rect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CaptureTarget {
    #[serde(rename_all = "camelCase")]
    Display { display_id: u32 },
    #[serde(rename_all = "camelCase")]
    Window { window_id: u32 },
    /// `rect` is in points, relative to the display's top-left corner.
    #[serde(rename_all = "camelCase")]
    Region { display_id: u32, rect: Rect },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CaptureOptions {
    pub target: CaptureTarget,
    pub fps: u32,
    pub system_audio: bool,
    pub mic: bool,
}

#[derive(Debug, Clone)]
pub struct OutputFiles {
    pub video: PathBuf,
    pub system_audio: PathBuf,
    pub mic: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaptureInfo {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    /// Captured area in global display points.
    pub bounds: Rect,
    pub fps: u32,
    pub codec: String,
    pub window_title: Option<String>,
    pub app_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Track {
    Video,
    SystemAudio,
    Mic,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CaptureEvent {
    /// The first sample of a track was written; `host_ns` is its presentation time on the host clock.
    TrackStarted {
        track: Track,
        host_ns: u64,
    },
    Failed {
        message: String,
    },
}

pub type EventHandler = Arc<dyn Fn(CaptureEvent) + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("screen recording permission is missing")]
    PermissionDenied,
    #[error("{0} not found")]
    NotFound(String),
    #[error("invalid region: {0}")]
    InvalidRegion(String),
    #[error("{0}")]
    Platform(String),
    #[error("screen capture is not supported on this platform")]
    Unsupported,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub trait ScreenCapture: Send + Sync {
    fn displays(&self) -> Result<Vec<DisplayInfo>>;
    fn windows(&self) -> Result<Vec<WindowInfo>>;
    fn start(
        &self,
        options: &CaptureOptions,
        files: &OutputFiles,
        on_event: EventHandler,
    ) -> Result<Box<dyn ActiveCapture>>;
}

pub trait ActiveCapture: Send {
    fn info(&self) -> &CaptureInfo;
    /// Stops capturing and finishes every output file.
    fn stop(self: Box<Self>) -> Result<()>;
}

#[cfg(target_os = "macos")]
pub fn platform() -> impl ScreenCapture {
    macos::MacCapture
}

/// HEVC bitrate for the raw recording, kept high because it is re-encoded on export.
pub fn video_bitrate(width: u32, height: u32, fps: u32) -> u32 {
    let bits = width as f64 * height as f64 * fps as f64 * 0.12;
    bits.clamp(10_000_000.0, 160_000_000.0) as u32
}

/// Rounds a pixel size down to an even number, as 4:2:0 video requires.
pub fn even(value: f64) -> u32 {
    let v = value.round().max(2.0) as u32;
    v - v % 2
}

pub fn check_region(display: &Rect, region: &Rect) -> Result<()> {
    let within = region.x >= 0.0
        && region.y >= 0.0
        && region.x + region.width <= display.width
        && region.y + region.height <= display.height;
    if region.width < 16.0 || region.height < 16.0 {
        return Err(Error::InvalidRegion("must be at least 16×16 points".into()));
    }
    if !within {
        return Err(Error::InvalidRegion(format!(
            "must fit inside the {}×{} display",
            display.width, display.height
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn even_rounds_down_to_even() {
        assert_eq!(even(1919.6), 1920);
        assert_eq!(even(1081.0), 1080);
        assert_eq!(even(0.2), 2);
    }

    #[test]
    fn bitrate_is_clamped() {
        assert_eq!(video_bitrate(320, 240, 60), 10_000_000);
        assert_eq!(video_bitrate(3024, 1964, 60), 42_761_779);
        assert_eq!(video_bitrate(7680, 4320, 60), 160_000_000);
    }

    #[test]
    fn region_must_fit_display() {
        let display = Rect {
            x: 0.0,
            y: 0.0,
            width: 1512.0,
            height: 982.0,
        };
        let ok = Rect {
            x: 10.0,
            y: 10.0,
            width: 800.0,
            height: 600.0,
        };
        assert!(check_region(&display, &ok).is_ok());
        let outside = Rect { x: 1000.0, ..ok };
        assert!(check_region(&display, &outside).is_err());
        let tiny = Rect { width: 4.0, ..ok };
        assert!(check_region(&display, &tiny).is_err());
    }

    #[test]
    fn target_json_shape() {
        let json = serde_json::to_string(&CaptureTarget::Region {
            display_id: 2,
            rect: Rect {
                x: 1.0,
                y: 2.0,
                width: 3.0,
                height: 4.0,
            },
        })
        .unwrap();
        assert!(json.contains("\"kind\":\"region\""));
        assert!(json.contains("\"displayId\":2"));
    }
}
