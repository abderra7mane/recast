//! Spring motion sampled on a fixed time grid. Each track is simulated once, from
//! time zero, so any time can be looked up and always gives the same result.

use recast_project::ZoomSegment;

use crate::{Camera, Point, clamp_center, focus_target, segments::LEAD_MS, spring};

/// 240 steps per second.
pub const STEP_MS: f64 = 1000.0 / 240.0;
/// Before a recorded position, the cursor moves from the previous one for at most this long.
const MAX_INTERPOLATION_MS: f64 = 50.0;
const ZOOM_OMEGA: f64 = 7.0;
const PAN_OMEGA: f64 = 7.0;
const SMOOTHING_OMEGA_MAX: f64 = 60.0;
const SMOOTHING_OMEGA_MIN: f64 = 6.0;
/// While following, the cursor moves freely inside this central fraction of the view.
const DEAD_ZONE: f64 = 0.5;

fn steps(duration_ms: f64) -> usize {
    (duration_ms / STEP_MS).ceil() as usize + 1
}

/// Index and blend factor for looking `t_ms` up on the grid.
fn grid(t_ms: f64, len: usize) -> (usize, f64) {
    let last = len.saturating_sub(1);
    let pos = (t_ms / STEP_MS).clamp(0.0, last as f64);
    let i = (pos.floor() as usize).min(last.saturating_sub(1));
    (i, pos - i as f64)
}

fn lerp(a: f64, b: f64, f: f64) -> f64 {
    a + (b - a) * f
}

/// The recorded cursor positions, interpolated between samples.
#[derive(Debug, Clone)]
pub struct CursorPath {
    points: Vec<(f64, Point)>,
}

impl CursorPath {
    pub fn new(points: Vec<(f64, Point)>) -> Self {
        Self { points }
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn at(&self, t_ms: f64) -> Point {
        let i = self.points.partition_point(|(t, _)| *t <= t_ms);
        let Some(&(_, before)) = self.points.get(i.wrapping_sub(1)) else {
            return self.points.first().map_or(Point::CENTER, |p| p.1);
        };
        let Some(&(next_t, next)) = self.points.get(i) else {
            return before;
        };
        let window = (next_t - self.points[i - 1].0).min(MAX_INTERPOLATION_MS);
        let f = ((t_ms - (next_t - window)) / window.max(1e-9)).clamp(0.0, 1.0);
        Point::new(lerp(before.x, next.x, f), lerp(before.y, next.y, f))
    }
}

#[derive(Debug, Clone)]
pub struct CursorTrack {
    samples: Vec<Point>,
}

impl CursorTrack {
    /// `smoothing` 0 keeps the recorded path; 1 is the heaviest smoothing.
    pub fn build(path: &CursorPath, smoothing: f64, duration_ms: f64) -> Self {
        let n = steps(duration_ms);
        let at = |k: usize| path.at(k as f64 * STEP_MS);
        if smoothing <= 0.0 {
            return Self {
                samples: (0..n).map(at).collect(),
            };
        }
        let omega =
            SMOOTHING_OMEGA_MAX * (SMOOTHING_OMEGA_MIN / SMOOTHING_OMEGA_MAX).powf(smoothing);
        let dt = STEP_MS / 1000.0;
        let mut p = at(0);
        let (mut vx, mut vy) = (0.0, 0.0);
        let mut samples = Vec::with_capacity(n);
        for k in 0..n {
            if k > 0 {
                let target = at(k);
                let (x, nvx) = spring::step(p.x, vx, target.x, omega, dt);
                let (y, nvy) = spring::step(p.y, vy, target.y, omega, dt);
                p = Point::new(x, y);
                (vx, vy) = (nvx, nvy);
            }
            samples.push(p);
        }
        Self { samples }
    }

    pub fn sample(&self, t_ms: f64) -> Point {
        let (i, f) = grid(t_ms, self.samples.len());
        let a = self.samples[i];
        let b = self.samples.get(i + 1).copied().unwrap_or(a);
        Point::new(lerp(a.x, b.x, f), lerp(a.y, b.y, f))
    }
}

#[derive(Debug, Clone, Copy)]
struct CameraSample {
    log_scale: f64,
    center: Point,
}

#[derive(Debug, Clone)]
pub struct CameraTrack {
    samples: Vec<CameraSample>,
}

/// Moves `target` just enough that `cursor` is inside the dead zone of a view of `scale`.
fn follow(target: Point, cursor: Point, scale: f64) -> Point {
    let half = DEAD_ZONE * 0.5 / scale;
    let axis = |t: f64, c: f64| t.clamp(c - half, c + half);
    clamp_center(
        Point::new(axis(target.x, cursor.x), axis(target.y, cursor.y)),
        scale,
    )
}

impl CameraTrack {
    pub fn build(segments: &[ZoomSegment], path: &CursorPath, duration_ms: f64) -> Self {
        let n = steps(duration_ms);
        let dt = STEP_MS / 1000.0;
        let mut z = (0.0, 0.0);
        let mut x = (0.5, 0.0);
        let mut y = (0.5, 0.0);
        let mut active: Option<usize> = None;
        let mut target_center = Point::CENTER;
        let mut samples = Vec::with_capacity(n);

        for k in 0..n {
            let t = k as f64 * STEP_MS;
            let started = segments.partition_point(|s| s.start_ms <= t);
            let current = segments[..started].iter().rposition(|s| t < s.end_ms);
            let (target_scale, target) = match current.map(|i| (i, &segments[i])) {
                Some((i, segment)) => {
                    let cursor = if path.is_empty() {
                        Point::CENTER
                    } else {
                        path.at(t)
                    };
                    if active != Some(i) {
                        let ahead = if path.is_empty() {
                            Point::CENTER
                        } else {
                            path.at((segment.start_ms + LEAD_MS).min(segment.end_ms))
                        };
                        target_center = clamp_center(ahead, segment.level);
                    }
                    if t >= segment.start_ms + LEAD_MS {
                        target_center = follow(target_center, cursor, segment.level);
                    }
                    let goal = clamp_center(focus_target(segment, target_center), segment.level);
                    (segment.level, goal)
                }
                None => (1.0, Point::CENTER),
            };
            active = current;

            if k > 0 {
                z = spring::step(z.0, z.1, target_scale.ln(), ZOOM_OMEGA, dt);
                x = spring::step(x.0, x.1, target.x, PAN_OMEGA, dt);
                y = spring::step(y.0, y.1, target.y, PAN_OMEGA, dt);
            }
            samples.push(CameraSample {
                log_scale: z.0,
                center: Point::new(x.0, y.0),
            });
        }
        Self { samples }
    }

    pub fn sample(&self, t_ms: f64) -> Camera {
        let (i, f) = grid(t_ms, self.samples.len());
        let a = self.samples[i];
        let b = self.samples.get(i + 1).copied().unwrap_or(a);
        let scale = lerp(a.log_scale, b.log_scale, f).exp().max(1.0);
        let center = Point::new(
            lerp(a.center.x, b.center.x, f),
            lerp(a.center.y, b.center.y, f),
        );
        Camera {
            scale,
            center: clamp_center(center, scale),
        }
    }
}
