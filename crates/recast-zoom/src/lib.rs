//! Motion for a recording: zoom segments, the camera, the smoothed cursor and click
//! effects. Everything is a pure function of the events, the settings and time, so
//! the preview and the export show the same frame for the same time.

mod segments;
pub mod spring;
mod track;

use recast_project::{
    EditSettings, EventKind, EventLog, MouseButton, Rect, ZoomFocus, ZoomSegment,
};

pub use segments::{GROUP_GAP_MS, IDLE_MS, LEAD_MS, MIN_GAP_MS, auto_segments};
use spring::smoothstep;
use track::{CameraTrack, CursorPath, CursorTrack};

/// A position on the screen, normalized so the recorded area spans 0..1 from its
/// top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const CENTER: Point = Point { x: 0.5, y: 0.5 };

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// The visible part of the screen: `scale` 1 shows all of it, 2 shows a quarter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub scale: f64,
    pub center: Point,
}

impl Camera {
    pub const FULL: Camera = Camera {
        scale: 1.0,
        center: Point::CENTER,
    };

    /// Normalized `(x, y, width, height)` of the visible area.
    pub fn view(&self) -> (f64, f64, f64, f64) {
        let size = 1.0 / self.scale;
        (
            self.center.x - size / 2.0,
            self.center.y - size / 2.0,
            size,
            size,
        )
    }

    /// Maps a screen position to its place in the view (0..1 when visible).
    pub fn project(&self, p: Point) -> Point {
        let (x, y, w, h) = self.view();
        Point::new((p.x - x) / w, (p.y - y) / h)
    }
}

/// Keeps a view of `scale` inside the screen.
pub fn clamp_center(center: Point, scale: f64) -> Point {
    let half = 0.5 / scale.max(1.0);
    Point::new(
        center.x.clamp(half, 1.0 - half),
        center.y.clamp(half, 1.0 - half),
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Click {
    pub t_ms: f64,
    pub pos: Point,
    pub button: MouseButton,
    /// `true` for a press, `false` for a release.
    pub down: bool,
}

impl Click {
    /// Whether the click landed inside the recorded area; clicks in other windows
    /// or on other displays do not zoom, ripple or make a sound.
    pub fn on_screen(&self) -> bool {
        (0.0..=1.0).contains(&self.pos.x) && (0.0..=1.0).contains(&self.pos.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorState {
    pub pos: Point,
    /// Recorded shape id; `None` when no shape was recorded.
    pub shape: Option<u32>,
    pub opacity: f64,
    /// 0 when released, 1 while held down; eases in between.
    pub press: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ripple {
    pub pos: Point,
    /// 0 at the click, 1 when the ripple has faded out.
    pub progress: f64,
}

pub const RIPPLE_MS: f64 = 550.0;
const PRESS_IN_MS: f64 = 60.0;
const PRESS_OUT_MS: f64 = 160.0;
/// With hide-when-idle, the cursor fades out after this long without input.
pub const CURSOR_IDLE_MS: f64 = 2_000.0;
const CURSOR_FADE_MS: f64 = 250.0;
/// Movements smaller than this (in normalized units) do not count as activity.
const STILL: f64 = 1e-4;

#[derive(Debug, Clone)]
pub struct Timeline {
    duration_ms: f64,
    segments: Vec<ZoomSegment>,
    camera: CameraTrack,
    cursor: Option<CursorTrack>,
    clicks: Vec<Click>,
    presses: Vec<(f64, f64)>,
    activity: Vec<f64>,
    shapes: Vec<(f64, u32)>,
    hide_when_idle: bool,
}

impl Timeline {
    /// `bounds` is the recorded area in the same global points as the events.
    pub fn new(log: &EventLog, bounds: &Rect, settings: &EditSettings, duration_ms: f64) -> Self {
        let duration_ms = duration_ms.max(0.0);
        let settings = settings.sanitized(duration_ms);
        let normalize = |x: f64, y: f64| {
            Point::new(
                (x - bounds.x) / bounds.width.max(1e-9),
                (y - bounds.y) / bounds.height.max(1e-9),
            )
        };

        let mut positions = Vec::new();
        let mut clicks = Vec::new();
        let mut shapes = Vec::new();
        let mut activity = Vec::new();
        for event in &log.events {
            let t = event.t_ms;
            let (x, y) = match &event.kind {
                EventKind::Cursor { shape } => {
                    shapes.push((t, *shape));
                    continue;
                }
                EventKind::Move { x, y }
                | EventKind::Drag { x, y, .. }
                | EventKind::Down { x, y, .. }
                | EventKind::Up { x, y, .. }
                | EventKind::Scroll { x, y, .. } => (*x, *y),
            };
            let pos = normalize(x, y);
            let moved = positions.last().is_none_or(|&(_, last): &(f64, Point)| {
                (last.x - pos.x).abs() > STILL || (last.y - pos.y).abs() > STILL
            });
            match &event.kind {
                EventKind::Down { button, .. } | EventKind::Up { button, .. } => {
                    clicks.push(Click {
                        t_ms: t,
                        pos,
                        button: *button,
                        down: matches!(event.kind, EventKind::Down { .. }),
                    });
                    activity.push(t);
                }
                EventKind::Scroll { .. } => activity.push(t),
                _ if moved => activity.push(t),
                _ => {}
            }
            positions.push((t, pos));
        }

        let segments = if settings.zoom.auto {
            auto_segments(&clicks, settings.zoom.level, duration_ms)
        } else {
            settings.zoom.segments.clone()
        };
        let path = CursorPath::new(positions);
        let camera = CameraTrack::build(&segments, &path, duration_ms);
        let cursor = (!path.is_empty())
            .then(|| CursorTrack::build(&path, settings.cursor.smoothing, duration_ms));

        Self {
            duration_ms,
            segments,
            camera,
            cursor,
            presses: press_intervals(&clicks),
            clicks,
            activity,
            shapes,
            hide_when_idle: settings.cursor.hide_when_idle,
        }
    }

    pub fn duration_ms(&self) -> f64 {
        self.duration_ms
    }

    /// The zoom segments in effect, generated or user-defined.
    pub fn segments(&self) -> &[ZoomSegment] {
        &self.segments
    }

    pub fn clicks(&self) -> &[Click] {
        &self.clicks
    }

    pub fn camera(&self, t_ms: f64) -> Camera {
        self.camera.sample(t_ms)
    }

    pub fn cursor(&self, t_ms: f64) -> Option<CursorState> {
        let track = self.cursor.as_ref()?;
        Some(CursorState {
            pos: track.sample(t_ms),
            shape: self.shape_at(t_ms),
            opacity: self.cursor_opacity(t_ms),
            press: self.press(t_ms),
        })
    }

    /// Ripples visible at `t_ms`, oldest first.
    pub fn ripples(&self, t_ms: f64) -> impl Iterator<Item = Ripple> + '_ {
        let first = self.clicks.partition_point(|c| c.t_ms < t_ms - RIPPLE_MS);
        self.clicks[first..]
            .iter()
            .take_while(move |c| c.t_ms <= t_ms)
            .filter(|c| c.down && c.on_screen())
            .map(move |c| Ripple {
                pos: c.pos,
                progress: ((t_ms - c.t_ms) / RIPPLE_MS).clamp(0.0, 1.0),
            })
            .filter(|r| r.progress < 1.0)
    }

    fn shape_at(&self, t_ms: f64) -> Option<u32> {
        let i = self.shapes.partition_point(|(t, _)| *t <= t_ms);
        self.shapes
            .get(i.saturating_sub(1))
            .map(|(_, shape)| *shape)
    }

    fn press(&self, t_ms: f64) -> f64 {
        let i = self.presses.partition_point(|(down, _)| *down <= t_ms);
        let Some(&(down, up)) = self.presses.get(i.wrapping_sub(1)) else {
            return 0.0;
        };
        let rise = |t: f64| smoothstep((t - down) / PRESS_IN_MS);
        if t_ms < up {
            rise(t_ms)
        } else {
            rise(up) * (1.0 - smoothstep((t_ms - up) / PRESS_OUT_MS))
        }
    }

    fn cursor_opacity(&self, t_ms: f64) -> f64 {
        if !self.hide_when_idle {
            return 1.0;
        }
        let i = self.activity.partition_point(|t| *t <= t_ms);
        let last = i.checked_sub(1).map_or(0.0, |j| self.activity[j]).max(0.0);
        let fade_out = smoothstep((t_ms - last - CURSOR_IDLE_MS) / CURSOR_FADE_MS);
        let fade_in = self
            .activity
            .get(i)
            .map_or(0.0, |next| smoothstep(1.0 - (next - t_ms) / CURSOR_FADE_MS));
        (1.0 - fade_out).max(fade_in)
    }
}

/// `(down, up)` times; a press without a release lasts forever.
fn press_intervals(clicks: &[Click]) -> Vec<(f64, f64)> {
    let mut presses: Vec<(f64, f64)> = Vec::new();
    let mut open: Option<(f64, MouseButton)> = None;
    for click in clicks {
        match (click.down, open) {
            (true, _) => {
                if let Some((down, _)) = open {
                    presses.push((down, click.t_ms));
                }
                open = Some((click.t_ms, click.button));
            }
            (false, Some((down, button))) if button == click.button => {
                presses.push((down, click.t_ms));
                open = None;
            }
            _ => {}
        }
    }
    if let Some((down, _)) = open {
        presses.push((down, f64::INFINITY));
    }
    presses
}

/// The target view for a point in time; used by the camera simulation.
fn focus_target(segment: &ZoomSegment, follow: Point) -> Point {
    match segment.focus {
        ZoomFocus::FollowCursor => follow,
        ZoomFocus::Point { x, y } => Point::new(x, y),
    }
}

#[cfg(test)]
mod tests;
