use std::{path::PathBuf, sync::Arc};

use serde::{Deserialize, Serialize};
use specta::Type;

#[cfg(target_os = "macos")]
pub mod macos;
pub mod picker;

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

/// A still image of a capture target at the display's native resolution, without the cursor.
#[derive(Debug, Clone, PartialEq)]
pub struct Screenshot {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    /// Straight (not premultiplied) sRGB RGBA, tightly packed. Window captures keep
    /// their transparent corners and have no system shadow.
    pub rgba: Vec<u8>,
}

pub trait ScreenCapture: Send + Sync {
    fn displays(&self) -> Result<Vec<DisplayInfo>>;
    fn windows(&self) -> Result<Vec<WindowInfo>>;
    /// On-screen windows ordered front to back, for hit-testing in the target picker.
    fn window_stack(&self) -> Result<Vec<picker::PickerWindow>> {
        Err(Error::Unsupported)
    }
    fn screenshot(&self, _target: &CaptureTarget) -> Result<Screenshot> {
        Err(Error::Unsupported)
    }
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

/// HEVC bitrate for the raw recording. It is re-encoded on export, and screen text
/// only stays sharp with enough bits for the first frame and every big change.
pub fn video_bitrate(width: u32, height: u32, fps: u32) -> u32 {
    let bits = width as f64 * height as f64 * fps as f64 * 0.2;
    bits.clamp(20_000_000.0, 240_000_000.0) as u32
}

/// An area of the screen that ScreenCaptureKit delivers without scaling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoArea {
    /// In points, on whole pixels.
    pub rect: Rect,
    pub width: u32,
    pub height: u32,
}

/// The part of `rect` (points) that maps 1:1 to video pixels at `scale`. 4:2:0 video
/// needs even sizes, so an odd size loses its last pixel column or row: any other output
/// size makes ScreenCaptureKit resample, and that blurs every pixel.
pub fn video_area(rect: &Rect, scale: f64) -> VideoArea {
    let px = |v: f64| (v * scale).round();
    let (x, y) = (px(rect.x), px(rect.y));
    let even = |size: f64| {
        let size = size.max(2.0) as u32;
        size - size % 2
    };
    let width = even(px(rect.x + rect.width) - x);
    let height = even(px(rect.y + rect.height) - y);
    VideoArea {
        rect: Rect {
            x: x / scale,
            y: y / scale,
            width: width as f64 / scale,
            height: height as f64 / scale,
        },
        width,
        height,
    }
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

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn video_area_keeps_even_pixel_sizes() {
        let area = video_area(&rect(864.0, 216.0, 1368.0, 954.0), 1.0);
        assert_eq!((area.width, area.height), (1368, 954));
        assert_eq!(area.rect, rect(864.0, 216.0, 1368.0, 954.0));
        let area = video_area(&rect(0.0, 0.0, 1728.0, 1117.0), 2.0);
        assert_eq!((area.width, area.height), (3456, 2234));
    }

    #[test]
    fn video_area_crops_odd_pixel_sizes_instead_of_scaling() {
        let area = video_area(&rect(864.0, 216.0, 1369.0, 955.0), 1.0);
        assert_eq!((area.width, area.height), (1368, 954));
        assert_eq!(area.rect, rect(864.0, 216.0, 1368.0, 954.0));

        let area = video_area(&rect(10.5, 20.0, 300.5, 200.0), 2.0);
        assert_eq!((area.width, area.height), (600, 400));
        assert_eq!(area.rect, rect(10.5, 20.0, 300.0, 200.0));

        let area = video_area(&rect(0.0, 0.0, 1920.0, 1081.0), 1.0);
        assert_eq!((area.width, area.height), (1920, 1080));
    }

    #[test]
    fn video_area_maps_points_to_whole_pixels() {
        for scale in [1.0, 1.5, 2.0, 3.0] {
            for i in 0..200 {
                let r = rect(
                    i as f64 * 1.37,
                    i as f64 * 0.71,
                    40.0 + i as f64 * 3.3,
                    31.7 + i as f64,
                );
                let area = video_area(&r, scale);
                assert_eq!(area.width % 2, 0);
                assert_eq!(area.height % 2, 0);
                assert!((area.rect.width * scale - area.width as f64).abs() < 1e-9);
                assert!((area.rect.height * scale - area.height as f64).abs() < 1e-9);
                assert!(((area.rect.x * scale).round() - area.rect.x * scale).abs() < 1e-9);
                assert!(area.rect.x + area.rect.width <= r.x + r.width + 0.5 / scale + 1e-9);
            }
        }
    }

    #[test]
    fn bitrate_is_clamped() {
        assert_eq!(video_bitrate(320, 240, 60), 20_000_000);
        assert_eq!(video_bitrate(3024, 1964, 60), 71_269_632);
        assert_eq!(video_bitrate(7680, 4320, 60), 240_000_000);
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
