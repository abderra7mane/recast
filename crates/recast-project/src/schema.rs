use serde::{Deserialize, Serialize};
use specta::Type;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CaptureSource {
    #[serde(rename_all = "camelCase")]
    Display { display_id: u32 },
    #[serde(rename_all = "camelCase")]
    Window {
        window_id: u32,
        title: Option<String>,
        app_name: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Region { display_id: u32, rect: Rect },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VideoTrack {
    pub file: String,
    pub codec: String,
    pub duration_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AudioTrack {
    pub file: String,
    /// Start of the track relative to the first video frame.
    pub offset_ms: f64,
    pub duration_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Recording {
    pub source: CaptureSource,
    /// Captured area in global display points; input events use the same space.
    pub bounds: Rect,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub fps: u32,
    pub duration_ms: f64,
    pub video: VideoTrack,
    pub system_audio: Option<AudioTrack>,
    pub mic: Option<AudioTrack>,
    pub events_file: String,
    pub cursors_dir: String,
    pub recovered: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub version: u32,
    pub name: String,
    pub created_at_unix_ms: f64,
    pub recording: Recording,
}
