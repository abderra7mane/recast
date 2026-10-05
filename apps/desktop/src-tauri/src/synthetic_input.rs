//! An `InputCapture` that plays a scripted mouse session (moves, clicks, a right
//! click and a double click), for exercising zoom and click effects without
//! Input Monitoring permission.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use recast_capture::macos::now_host_ns;
use recast_input::{ActiveInput, InputCapture, InputRecord, InputSink, Result};
use recast_project::{CursorShape, EventKind, MouseButton};

/// The script repeats every this many seconds.
const PERIOD: f64 = 8.0;
const TICK: Duration = Duration::from_millis(8);

/// Cursor positions over one period, in a 640×360 space.
const PATH: [(f64, f64, f64); 9] = [
    (0.0, 120.0, 90.0),
    (0.6, 120.0, 90.0),
    (1.4, 210.0, 130.0),
    (2.6, 250.0, 150.0),
    (3.4, 250.0, 150.0),
    (4.4, 500.0, 270.0),
    (6.0, 500.0, 270.0),
    (7.0, 320.0, 180.0),
    (8.0, 120.0, 90.0),
];

/// `(time, button, click count)` of each press; releases follow 90 ms later.
const CLICKS: [(f64, MouseButton, u32); 6] = [
    (1.6, MouseButton::Left, 1),
    (2.2, MouseButton::Left, 1),
    (2.9, MouseButton::Right, 1),
    (4.8, MouseButton::Left, 1),
    (5.2, MouseButton::Left, 1),
    (5.35, MouseButton::Left, 2),
];
const RELEASE_AFTER: f64 = 0.09;

fn position(t: f64) -> (f64, f64) {
    let t = t.rem_euclid(PERIOD);
    let i = PATH.iter().rposition(|k| k.0 <= t).unwrap_or(0);
    let (t0, x0, y0) = PATH[i];
    let (t1, x1, y1) = PATH[(i + 1).min(PATH.len() - 1)];
    let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
    let eased = f * f * (3.0 - 2.0 * f);
    (x0 + (x1 - x0) * eased, y0 + (y1 - y0) * eased)
}

/// Button events of one period, sorted by time.
fn button_events() -> Vec<(f64, MouseButton, Option<u32>)> {
    let mut events: Vec<_> = CLICKS
        .iter()
        .flat_map(|&(t, button, count)| {
            [(t, button, Some(count)), (t + RELEASE_AFTER, button, None)]
        })
        .collect();
    events.sort_by(|a, b| a.0.total_cmp(&b.0));
    events
}

pub struct SyntheticInput {
    /// Size of the recorded display in points.
    pub width: f64,
    pub height: f64,
}

impl InputCapture for SyntheticInput {
    fn start(
        &self,
        cursors_dir: PathBuf,
        relative_dir: String,
        sink: InputSink,
    ) -> Result<Box<dyn ActiveInput>> {
        let scale = 2.0;
        let (w, h, pixels, hotspot_x, hotspot_y) =
            recast_render::cursor::default_arrow_straight(scale);
        recast_render::bitmap::save_png(&cursors_dir.join("0.png"), w, h, &pixels)
            .map_err(|e| recast_input::Error::Platform(e.to_string()))?;
        sink(InputRecord::Shape(CursorShape {
            id: 0,
            file: format!("{relative_dir}/0.png"),
            hash: "synthetic-arrow".into(),
            hotspot_x,
            hotspot_y,
            width: w as f64 / scale,
            height: h as f64 / scale,
            scale,
        }));
        let start = now_host_ns();
        sink(InputRecord::Event {
            host_ns: start,
            kind: EventKind::Cursor { shape: 0 },
        });

        let (sx, sy) = (self.width / 640.0, self.height / 360.0);
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = thread::spawn(move || {
            let buttons = button_events();
            let mut next_button = 0;
            let mut period = 0.0;
            while !flag.load(Ordering::Acquire) {
                let now = now_host_ns();
                let t = (now - start) as f64 / 1e9;
                let at = |t: f64| {
                    let (x, y) = position(t);
                    (x * sx, y * sy)
                };
                while let Some(&(bt, button, count)) = buttons.get(next_button)
                    && period + bt <= t
                {
                    let (x, y) = at(period + bt);
                    let kind = match count {
                        Some(click_count) => EventKind::Down {
                            x,
                            y,
                            button,
                            click_count,
                        },
                        None => EventKind::Up { x, y, button },
                    };
                    let host_ns = start + ((period + bt) * 1e9) as u64;
                    sink(InputRecord::Event { host_ns, kind });
                    next_button += 1;
                    if next_button == buttons.len() {
                        next_button = 0;
                        period += PERIOD;
                    }
                }
                let (x, y) = at(t);
                sink(InputRecord::Event {
                    host_ns: now,
                    kind: EventKind::Move { x, y },
                });
                thread::sleep(TICK);
            }
        });
        Ok(Box::new(SyntheticActive {
            stop,
            thread: Some(thread),
        }))
    }
}

struct SyntheticActive {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ActiveInput for SyntheticActive {
    fn listening(&self) -> bool {
        true
    }

    fn stop(mut self: Box<Self>) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_is_continuous_and_repeats() {
        assert_eq!(position(0.0), (120.0, 90.0));
        assert_eq!(position(PERIOD), position(0.0));
        let (x, y) = position(1.0);
        assert!(x > 120.0 && x < 210.0 && y > 90.0 && y < 130.0);
        assert_eq!(position(5.0), (500.0, 270.0));
    }

    #[test]
    fn every_press_is_released() {
        let events = button_events();
        let presses = events.iter().filter(|e| e.2.is_some()).count();
        assert_eq!(presses * 2, events.len());
        assert!(events.windows(2).all(|w| w[0].0 <= w[1].0));
    }
}
