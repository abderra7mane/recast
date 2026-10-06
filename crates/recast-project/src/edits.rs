use std::fmt;

use serde::{Deserialize, Serialize};
use specta::Type;

pub const EDITS_VERSION: u32 = 1;

/// An sRGB color, written as `#rrggbb` or `#rrggbbaa`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(try_from = "String", into = "String")]
#[specta(type = String)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn to_f32(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a].map(|c| c as f32 / 255.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid color {0:?}, expected #rrggbb or #rrggbbaa")]
pub struct InvalidColor(String);

impl TryFrom<String> for Color {
    type Error = InvalidColor;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let hex = value.strip_prefix('#').unwrap_or(&value);
        let digits: Option<Vec<u8>> = (0..hex.len())
            .step_by(2)
            .map(|i| {
                hex.get(i..i + 2)
                    .and_then(|pair| u8::from_str_radix(pair, 16).ok())
            })
            .collect();
        match digits.as_deref() {
            Some(&[r, g, b]) => Ok(Self::rgb(r, g, b)),
            Some(&[r, g, b, a]) => Ok(Self::rgba(r, g, b, a)),
            _ => Err(InvalidColor(value)),
        }
    }
}

impl From<Color> for String {
    fn from(c: Color) -> Self {
        c.to_string()
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)?;
        if self.a != 255 {
            write!(f, "{:02x}", self.a)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BackgroundFill {
    Solid {
        color: Color,
    },
    /// `angle_deg` follows CSS: 0 points up, 90 points right.
    #[serde(rename_all = "camelCase")]
    Gradient {
        from: Color,
        to: Color,
        angle_deg: f64,
    },
    /// PNG or JPEG, absolute or relative to the bundle. Scaled to cover the frame.
    Image {
        path: String,
    },
}

/// Sizes are fractions of the screen's shorter side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Shadow {
    pub opacity: f64,
    pub blur: f64,
    pub offset_y: f64,
}

impl Default for Shadow {
    fn default() -> Self {
        Self {
            opacity: 0.45,
            blur: 0.04,
            offset_y: 0.012,
        }
    }
}

/// `padding` and `corner_radius` are fractions of the screen's shorter side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct BackgroundSettings {
    pub fill: BackgroundFill,
    pub padding: f64,
    pub corner_radius: f64,
    pub shadow: Shadow,
}

impl Default for BackgroundSettings {
    fn default() -> Self {
        Self {
            fill: BackgroundFill::Gradient {
                from: Color::rgb(0x4f, 0x46, 0xe5),
                to: Color::rgb(0xdb, 0x27, 0x77),
                angle_deg: 135.0,
            },
            padding: 0.08,
            corner_radius: 0.015,
            shadow: Shadow::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct CursorSettings {
    /// Multiple of the recorded cursor size.
    pub size: f64,
    /// 0 follows the recorded path exactly; 1 is the smoothest.
    pub smoothing: f64,
    pub hide_when_idle: bool,
}

impl Default for CursorSettings {
    fn default() -> Self {
        Self {
            size: 1.5,
            smoothing: 0.5,
            hide_when_idle: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ZoomFocus {
    FollowCursor,
    /// Normalized position on the screen, 0..1 from the top-left corner.
    Point {
        x: f64,
        y: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ZoomSegment {
    pub start_ms: f64,
    pub end_ms: f64,
    pub level: f64,
    pub focus: ZoomFocus,
}

/// With `auto` on, segments are generated from clicks and `segments` is ignored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ZoomSettings {
    pub auto: bool,
    pub level: f64,
    pub segments: Vec<ZoomSegment>,
}

impl Default for ZoomSettings {
    fn default() -> Self {
        Self {
            auto: true,
            level: 2.0,
            segments: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ClickSettings {
    pub ripple: bool,
    pub color: Color,
    /// Largest ripple radius, in screen points.
    pub size: f64,
    pub squish: bool,
}

impl Default for ClickSettings {
    fn default() -> Self {
        Self {
            ripple: true,
            color: Color::rgba(0xff, 0xff, 0xff, 0xd9),
            size: 26.0,
            squish: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SoundPack {
    SoftTap,
    MouseClick,
    Mechanical,
    TrackpadTap,
    Pop,
    Tick,
}

impl SoundPack {
    pub const ALL: [SoundPack; 6] = [
        Self::MouseClick,
        Self::SoftTap,
        Self::Mechanical,
        Self::TrackpadTap,
        Self::Pop,
        Self::Tick,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::SoftTap => "soft-tap",
            Self::MouseClick => "mouse-click",
            Self::Mechanical => "mechanical",
            Self::TrackpadTap => "trackpad-tap",
            Self::Pop => "pop",
            Self::Tick => "tick",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct SoundSettings {
    pub enabled: bool,
    pub pack: SoundPack,
    /// 0..1
    pub volume: f64,
    /// Right clicks use the pack's own right-button sounds instead of the left ones.
    pub separate_left_right: bool,
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            pack: SoundPack::MouseClick,
            volume: 0.6,
            separate_left_right: false,
        }
    }
}

/// Gains from 0 (muted) to 2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct AudioMix {
    pub mic_volume: f64,
    pub system_volume: f64,
}

impl Default for AudioMix {
    fn default() -> Self {
        Self {
            mic_volume: 1.0,
            system_volume: 1.0,
        }
    }
}

/// Milliseconds on the recording's timeline; no `end_ms` means the end of the recording.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Trim {
    pub start_ms: f64,
    pub end_ms: Option<f64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Codec {
    #[default]
    H264,
    Hevc,
}

/// The output size: the recording at its own pixel size, or a preset for the output's
/// shorter side.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum Resolution {
    #[default]
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "1080p")]
    P1080,
    #[serde(rename = "1440p")]
    P1440,
    #[serde(rename = "4k")]
    P2160,
}

impl Resolution {
    /// The preset's shorter side; `None` for `Auto`.
    pub fn short_side(self) -> Option<u32> {
        match self {
            Self::Auto => None,
            Self::P1080 => Some(1080),
            Self::P1440 => Some(1440),
            Self::P2160 => Some(2160),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum FrameRate {
    #[serde(rename = "30")]
    Fps30,
    #[default]
    #[serde(rename = "60")]
    Fps60,
}

impl FrameRate {
    pub fn fps(self) -> u32 {
        match self {
            Self::Fps30 => 30,
            Self::Fps60 => 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ExportSettings {
    pub codec: Codec,
    pub resolution: Resolution,
    pub fps: FrameRate,
    /// 0..1, mapped to the encoder bitrate.
    pub quality: f64,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            codec: Codec::H264,
            resolution: Resolution::Auto,
            fps: FrameRate::Fps60,
            quality: 0.7,
        }
    }
}

/// Everything the user can change about a recording. Rendering never alters the
/// recorded media; these settings are applied at preview and export time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct EditSettings {
    pub version: u32,
    pub background: BackgroundSettings,
    pub cursor: CursorSettings,
    pub zoom: ZoomSettings,
    pub clicks: ClickSettings,
    pub sounds: SoundSettings,
    pub audio: AudioMix,
    pub trim: Trim,
    pub export: ExportSettings,
}

impl Default for EditSettings {
    fn default() -> Self {
        Self {
            version: EDITS_VERSION,
            background: BackgroundSettings::default(),
            cursor: CursorSettings::default(),
            zoom: ZoomSettings::default(),
            clicks: ClickSettings::default(),
            sounds: SoundSettings::default(),
            audio: AudioMix::default(),
            trim: Trim::default(),
            export: ExportSettings::default(),
        }
    }
}

fn clamp(value: f64, min: f64, max: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

impl EditSettings {
    pub const MAX_ZOOM: f64 = 6.0;

    /// Copy with every number inside its valid range and the trim inside `duration_ms`.
    pub fn sanitized(&self, duration_ms: f64) -> Self {
        let mut s = self.clone();
        let bg = &mut s.background;
        bg.padding = clamp(bg.padding, 0.0, 0.5, 0.0);
        bg.corner_radius = clamp(bg.corner_radius, 0.0, 0.5, 0.0);
        bg.shadow.opacity = clamp(bg.shadow.opacity, 0.0, 1.0, 0.0);
        bg.shadow.blur = clamp(bg.shadow.blur, 0.0, 0.5, 0.0);
        bg.shadow.offset_y = clamp(bg.shadow.offset_y, -0.5, 0.5, 0.0);
        if let BackgroundFill::Gradient { angle_deg, .. } = &mut bg.fill {
            *angle_deg = clamp(*angle_deg, -3600.0, 3600.0, 0.0);
        }

        s.cursor.size = clamp(s.cursor.size, 0.1, 10.0, 1.0);
        s.cursor.smoothing = clamp(s.cursor.smoothing, 0.0, 1.0, 0.0);
        s.zoom.level = clamp(s.zoom.level, 1.0, Self::MAX_ZOOM, 2.0);
        s.zoom
            .segments
            .retain(|z| z.start_ms.is_finite() && z.end_ms.is_finite() && z.end_ms > z.start_ms);
        for z in &mut s.zoom.segments {
            z.level = clamp(z.level, 1.0, Self::MAX_ZOOM, 2.0);
            if let ZoomFocus::Point { x, y } = &mut z.focus {
                *x = clamp(*x, 0.0, 1.0, 0.5);
                *y = clamp(*y, 0.0, 1.0, 0.5);
            }
        }
        s.zoom
            .segments
            .sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms));
        s.clicks.size = clamp(s.clicks.size, 0.0, 500.0, 0.0);
        s.sounds.volume = clamp(s.sounds.volume, 0.0, 1.0, 0.0);
        s.audio.mic_volume = clamp(s.audio.mic_volume, 0.0, 2.0, 1.0);
        s.audio.system_volume = clamp(s.audio.system_volume, 0.0, 2.0, 1.0);
        s.export.quality = clamp(s.export.quality, 0.0, 1.0, 0.7);

        let duration = duration_ms.max(0.0);
        let start = clamp(s.trim.start_ms, 0.0, duration, 0.0);
        let end = s
            .trim
            .end_ms
            .filter(|e| e.is_finite())
            .unwrap_or(duration)
            .clamp(start, duration);
        s.trim = Trim {
            start_ms: start,
            end_ms: Some(end),
        };
        s
    }

    /// Trimmed range as `(start, end)` milliseconds; call on sanitized settings.
    pub fn trim_range(&self, duration_ms: f64) -> (f64, f64) {
        let start = self.trim.start_ms;
        (start, self.trim.end_ms.unwrap_or(duration_ms).max(start))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_hex_round_trip() {
        let c: Color = serde_json::from_str("\"#4F46e5\"").unwrap();
        assert_eq!(c, Color::rgb(0x4f, 0x46, 0xe5));
        assert_eq!(serde_json::to_string(&c).unwrap(), "\"#4f46e5\"");
        let c: Color = serde_json::from_str("\"#ffffff80\"").unwrap();
        assert_eq!(c.a, 0x80);
        assert_eq!(c.to_string(), "#ffffff80");
        assert!(serde_json::from_str::<Color>("\"#fff\"").is_err());
        assert!(serde_json::from_str::<Color>("\"#gg0000\"").is_err());
        assert!(serde_json::from_str::<Color>("\"#ffé000\"").is_err());
    }

    #[test]
    fn json_shape() {
        let json = serde_json::to_value(EditSettings::default()).unwrap();
        assert_eq!(json["version"], EDITS_VERSION);
        assert_eq!(json["background"]["fill"]["kind"], "gradient");
        assert_eq!(json["background"]["fill"]["angleDeg"], 135.0);
        assert_eq!(json["export"]["codec"], "h264");
        assert_eq!(json["export"]["resolution"], "auto");
        assert_eq!(json["export"]["fps"], "60");
        assert_eq!(json["sounds"]["pack"], "mouseClick");
        assert_eq!(json["zoom"]["auto"], true);
        assert!(json["trim"]["endMs"].is_null());
    }

    #[test]
    fn missing_fields_take_defaults() {
        let s: EditSettings =
            serde_json::from_str(r#"{"zoom":{"level":3},"export":{"resolution":"4k"}}"#).unwrap();
        assert_eq!(s.zoom.level, 3.0);
        assert!(s.zoom.auto);
        assert_eq!(s.export.resolution, Resolution::P2160);
        assert_eq!(s.export.fps, FrameRate::Fps60);
        assert_eq!(s.cursor, CursorSettings::default());
        assert_eq!(s.version, EDITS_VERSION);
    }

    #[test]
    fn segments_round_trip() {
        let segment = ZoomSegment {
            start_ms: 100.0,
            end_ms: 900.0,
            level: 2.5,
            focus: ZoomFocus::Point { x: 0.25, y: 0.75 },
        };
        let json = serde_json::to_value(&segment).unwrap();
        assert_eq!(json["focus"]["kind"], "point");
        assert_eq!(
            serde_json::from_value::<ZoomSegment>(json).unwrap(),
            segment
        );
        let follow = serde_json::to_value(ZoomFocus::FollowCursor).unwrap();
        assert_eq!(follow["kind"], "followCursor");
    }

    #[test]
    fn sanitize_clamps_values_and_trim() {
        let mut s = EditSettings::default();
        s.background.padding = 4.0;
        s.cursor.smoothing = f64::NAN;
        s.zoom.level = 0.2;
        s.zoom.segments = vec![
            ZoomSegment {
                start_ms: 500.0,
                end_ms: 400.0,
                level: 2.0,
                focus: ZoomFocus::FollowCursor,
            },
            ZoomSegment {
                start_ms: 0.0,
                end_ms: 400.0,
                level: 99.0,
                focus: ZoomFocus::Point { x: -1.0, y: 2.0 },
            },
        ];
        s.trim = Trim {
            start_ms: -50.0,
            end_ms: Some(9_000.0),
        };
        let clean = s.sanitized(5_000.0);
        assert_eq!(clean.background.padding, 0.5);
        assert_eq!(clean.cursor.smoothing, 0.0);
        assert_eq!(clean.zoom.level, 1.0);
        assert_eq!(clean.zoom.segments.len(), 1);
        assert_eq!(clean.zoom.segments[0].level, EditSettings::MAX_ZOOM);
        assert_eq!(
            clean.zoom.segments[0].focus,
            ZoomFocus::Point { x: 0.0, y: 1.0 }
        );
        assert_eq!(clean.trim_range(5_000.0), (0.0, 5_000.0));

        let s = EditSettings {
            trim: Trim {
                start_ms: 3_000.0,
                end_ms: Some(1_000.0),
            },
            ..Default::default()
        };
        assert_eq!(s.sanitized(5_000.0).trim_range(5_000.0), (3_000.0, 3_000.0));
    }
}
