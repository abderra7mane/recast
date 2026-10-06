//! Geometry and interaction state of the target picker, independent of the UI toolkit.
//!
//! Coordinates are global display points with the origin at the main display's top-left
//! corner and y pointing down, as in Core Graphics and ScreenCaptureKit. AppKit puts the
//! origin at the main display's bottom-left corner with y pointing up; convert with
//! [`point_from_appkit`] and [`rect_from_appkit`].

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{CaptureTarget, Rect};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// A rectangle in whole pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// `main_height` is the height of the main display (the one with the menu bar) in points.
pub fn point_from_appkit(x: f64, y: f64, main_height: f64) -> Point {
    Point::new(x, main_height - y)
}

pub fn rect_from_appkit(rect: &Rect, main_height: f64) -> Rect {
    Rect {
        y: main_height - rect.y - rect.height,
        ..*rect
    }
}

pub fn rect_to_appkit(rect: &Rect, main_height: f64) -> Rect {
    rect_from_appkit(rect, main_height)
}

pub fn contains(rect: &Rect, p: Point) -> bool {
    p.x >= rect.x && p.x < rect.x + rect.width && p.y >= rect.y && p.y < rect.y + rect.height
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PickerDisplay {
    pub id: u32,
    pub name: String,
    pub bounds: Rect,
    pub scale_factor: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PickerWindow {
    pub id: u32,
    pub pid: i32,
    /// Core Graphics window level; 0 for normal windows.
    pub layer: i32,
    pub app_name: String,
    pub title: String,
    pub bounds: Rect,
}

/// Levels from the Dock's up hold system UI: the Dock, menu bar, status items.
const DOCK_LEVEL: i32 = 20;
const MIN_WINDOW_POINTS: f64 = 24.0;
/// How far the pointer moves before a press becomes a drag.
const DRAG_THRESHOLD: f64 = 3.0;

/// Whether a window can be picked: not ours, not desktop or system UI, not tiny.
pub fn pickable(window: &PickerWindow, own_pid: i32) -> bool {
    window.pid != own_pid
        && (0..DOCK_LEVEL).contains(&window.layer)
        && window.bounds.width >= MIN_WINDOW_POINTS
        && window.bounds.height >= MIN_WINDOW_POINTS
}

/// The frontmost pickable window under `p`; `windows` is ordered front to back.
pub fn window_at(windows: &[PickerWindow], p: Point, own_pid: i32) -> Option<&PickerWindow> {
    windows
        .iter()
        .find(|w| pickable(w, own_pid) && contains(&w.bounds, p))
}

pub fn display_at(displays: &[PickerDisplay], p: Point) -> Option<&PickerDisplay> {
    displays.iter().find(|d| contains(&d.bounds, p))
}

fn snap(value: f64, scale: f64) -> f64 {
    (value * scale).round() / scale
}

/// The rectangle spanned by a drag from `a` to `b`, in points relative to the display's
/// top-left corner, clamped to the display and snapped to its pixel grid.
pub fn region_between(display: &PickerDisplay, a: Point, b: Point) -> Rect {
    let d = &display.bounds;
    let scale = display.scale_factor.max(1.0);
    let local = |p: Point| {
        (
            snap((p.x - d.x).clamp(0.0, d.width), scale),
            snap((p.y - d.y).clamp(0.0, d.height), scale),
        )
    };
    let (ax, ay) = local(a);
    let (bx, by) = local(b);
    Rect {
        x: ax.min(bx),
        y: ay.min(by),
        width: (ax - bx).abs(),
        height: (ay - by).abs(),
    }
}

/// `rect` in points as whole pixels at `scale`.
pub fn to_pixels(rect: &Rect, scale: f64) -> PixelRect {
    let px = |v: f64| (v * scale).round().max(0.0) as u32;
    let (x0, y0) = (px(rect.x), px(rect.y));
    let (x1, y1) = (px(rect.x + rect.width), px(rect.y + rect.height));
    PixelRect {
        x: x0,
        y: y0,
        width: x1.saturating_sub(x0),
        height: y1.saturating_sub(y0),
    }
}

pub fn size_label(width: f64, height: f64) -> String {
    format!("{} × {}", width.round(), height.round())
}

/// What the picker picks. Each mode picks one kind of target only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PickMode {
    /// A drag selects a region; clicks do nothing.
    Area,
    /// The window under the pointer is highlighted; a click picks it.
    Window,
    /// The display under the pointer is highlighted; a click picks it.
    Display,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Picked {
    pub target: CaptureTarget,
    /// The display the target was picked on.
    pub display_id: u32,
    /// The picked area in global points.
    pub bounds: Rect,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Highlight {
    None,
    Window { bounds: Rect, label: String },
    Region { bounds: Rect, label: String },
    Display { bounds: Rect, label: String },
}

/// The pointer the picker shows on every display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerCursor {
    /// Window and display modes: a click picks what is under it.
    Camera,
    /// Area mode: a drag selects an area; a label next to it shows where it is or the
    /// selection size.
    Crosshair,
}

/// The text next to the crosshair: the pointer's position before a drag, the selection
/// size during one.
#[derive(Debug, Clone, PartialEq)]
pub struct PointerLabel {
    /// Index of the display it shows on.
    pub display: usize,
    /// The pointer in points relative to that display's top-left corner.
    pub at: Point,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Pending,
    Picked(Picked),
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    display: usize,
    start: Point,
    current: Point,
    moved: bool,
}

#[derive(Debug, Clone)]
pub struct Picker {
    displays: Vec<PickerDisplay>,
    windows: Vec<PickerWindow>,
    mode: PickMode,
    pointer: Option<Point>,
    drag: Option<Drag>,
}

fn window_label(window: &PickerWindow) -> String {
    let name = if window.app_name.is_empty() {
        &window.title
    } else {
        &window.app_name
    };
    format!(
        "{name}  {}",
        size_label(window.bounds.width, window.bounds.height)
    )
}

fn display_label(display: &PickerDisplay) -> String {
    format!(
        "{}  {}",
        display.name,
        size_label(display.bounds.width, display.bounds.height)
    )
}

impl Picker {
    /// `windows` is ordered front to back; windows that can't be picked are dropped.
    pub fn new(
        mode: PickMode,
        displays: Vec<PickerDisplay>,
        windows: Vec<PickerWindow>,
        own_pid: i32,
    ) -> Self {
        Self {
            displays,
            windows: windows
                .into_iter()
                .filter(|w| pickable(w, own_pid))
                .collect(),
            mode,
            pointer: None,
            drag: None,
        }
    }

    pub fn displays(&self) -> &[PickerDisplay] {
        &self.displays
    }

    pub fn mode(&self) -> PickMode {
        self.mode
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some_and(|d| d.moved)
    }

    /// Index of the display under `p`.
    pub fn display_index_at(&self, p: Point) -> Option<usize> {
        self.displays.iter().position(|d| contains(&d.bounds, p))
    }

    /// The same on every display, by mode.
    pub fn cursor(&self) -> PickerCursor {
        match self.mode {
            PickMode::Area => PickerCursor::Crosshair,
            PickMode::Window | PickMode::Display => PickerCursor::Camera,
        }
    }

    pub fn pointer_label(&self) -> Option<PointerLabel> {
        if self.cursor() != PickerCursor::Crosshair {
            return None;
        }
        let p = self.pointer?;
        if let Some(drag) = self.drag.filter(|d| d.moved) {
            let display = &self.displays[drag.display];
            let d = &display.bounds;
            let rect = region_between(display, drag.start, drag.current);
            return Some(PointerLabel {
                display: drag.display,
                at: Point::new(
                    (p.x - d.x).clamp(0.0, d.width),
                    (p.y - d.y).clamp(0.0, d.height),
                ),
                text: size_label(rect.width, rect.height),
            });
        }
        let index = self.display_index_at(p)?;
        let d = &self.displays[index].bounds;
        let at = Point::new(p.x - d.x, p.y - d.y);
        Some(PointerLabel {
            display: index,
            at,
            text: format!("{}, {}", at.x.floor(), at.y.floor()),
        })
    }

    pub fn move_to(&mut self, p: Point) {
        self.pointer = Some(p);
    }

    pub fn press(&mut self, p: Point) {
        self.pointer = Some(p);
        if self.mode != PickMode::Area {
            return;
        }
        self.drag = self
            .displays
            .iter()
            .position(|d| contains(&d.bounds, p))
            .map(|display| Drag {
                display,
                start: p,
                current: p,
                moved: false,
            });
    }

    pub fn drag_to(&mut self, p: Point) {
        self.pointer = Some(p);
        if let Some(drag) = &mut self.drag {
            drag.current = p;
            let distance = (p.x - drag.start.x).hypot(p.y - drag.start.y);
            drag.moved |= distance >= DRAG_THRESHOLD;
        }
    }

    pub fn release(&mut self, p: Point) -> Outcome {
        self.drag_to(p);
        match self.drag.take() {
            Some(drag) if drag.moved => self.region(&drag),
            _ => self.click(p),
        }
    }

    fn region(&self, drag: &Drag) -> Outcome {
        let display = &self.displays[drag.display];
        let rect = region_between(display, drag.start, drag.current);
        if to_pixels(&rect, display.scale_factor).width == 0
            || to_pixels(&rect, display.scale_factor).height == 0
        {
            return Outcome::Pending;
        }
        Outcome::Picked(Picked {
            target: CaptureTarget::Region {
                display_id: display.id,
                rect,
            },
            display_id: display.id,
            bounds: Rect {
                x: display.bounds.x + rect.x,
                y: display.bounds.y + rect.y,
                ..rect
            },
            label: size_label(rect.width, rect.height),
        })
    }

    fn click(&self, p: Point) -> Outcome {
        let Some(display) = display_at(&self.displays, p) else {
            return Outcome::Pending;
        };
        match self.mode {
            PickMode::Area => Outcome::Pending,
            PickMode::Window => match window_at(&self.windows, p, -1) {
                Some(window) => Outcome::Picked(Picked {
                    target: CaptureTarget::Window {
                        window_id: window.id,
                    },
                    display_id: display.id,
                    bounds: window.bounds,
                    label: window_label(window),
                }),
                None => Outcome::Pending,
            },
            PickMode::Display => Outcome::Picked(Picked {
                target: CaptureTarget::Display {
                    display_id: display.id,
                },
                display_id: display.id,
                bounds: display.bounds,
                label: display_label(display),
            }),
        }
    }

    pub fn highlight(&self) -> Highlight {
        if let Some(drag) = self.drag.filter(|d| d.moved) {
            let display = &self.displays[drag.display];
            let rect = region_between(display, drag.start, drag.current);
            return Highlight::Region {
                bounds: Rect {
                    x: display.bounds.x + rect.x,
                    y: display.bounds.y + rect.y,
                    ..rect
                },
                label: size_label(rect.width, rect.height),
            };
        }
        let Some(p) = self.pointer else {
            return Highlight::None;
        };
        match self.mode {
            PickMode::Area => Highlight::None,
            PickMode::Window => match window_at(&self.windows, p, -1) {
                Some(w) => Highlight::Window {
                    bounds: w.bounds,
                    label: window_label(w),
                },
                None => Highlight::None,
            },
            PickMode::Display => match display_at(&self.displays, p) {
                Some(d) => Highlight::Display {
                    bounds: d.bounds,
                    label: display_label(d),
                },
                None => Highlight::None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN_PID: i32 = 99;

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// A Retina main display and an external 1x display to its right, top-aligned.
    fn displays() -> Vec<PickerDisplay> {
        vec![
            PickerDisplay {
                id: 1,
                name: "Built-in".into(),
                bounds: rect(0.0, 0.0, 1512.0, 982.0),
                scale_factor: 2.0,
            },
            PickerDisplay {
                id: 2,
                name: "External".into(),
                bounds: rect(1512.0, 0.0, 1920.0, 1080.0),
                scale_factor: 1.0,
            },
        ]
    }

    fn window(id: u32, pid: i32, layer: i32, bounds: Rect) -> PickerWindow {
        PickerWindow {
            id,
            pid,
            layer,
            app_name: format!("App {id}"),
            title: String::new(),
            bounds,
        }
    }

    fn windows() -> Vec<PickerWindow> {
        vec![
            window(10, 1, 25, rect(0.0, 0.0, 1512.0, 24.0)),
            window(11, OWN_PID, 0, rect(100.0, 100.0, 400.0, 300.0)),
            window(12, 2, 0, rect(200.0, 150.0, 600.0, 400.0)),
            window(13, 3, 0, rect(50.0, 50.0, 1000.0, 800.0)),
            window(14, 4, 3, rect(1600.0, 100.0, 300.0, 200.0)),
            window(15, 5, 0, rect(900.0, 900.0, 10.0, 10.0)),
        ]
    }

    #[test]
    fn hit_test_takes_the_frontmost_pickable_window() {
        let ws = windows();
        let at = |x, y| window_at(&ws, Point::new(x, y), OWN_PID).map(|w| w.id);
        assert_eq!(at(150.0, 120.0), Some(13), "own window is skipped");
        assert_eq!(at(300.0, 200.0), Some(12), "front window wins");
        assert_eq!(at(10.0, 10.0), None, "menu bar is skipped");
        assert_eq!(at(60.0, 60.0), Some(13));
        assert_eq!(at(1700.0, 150.0), Some(14), "floating windows are pickable");
        assert_eq!(at(905.0, 905.0), None, "tiny windows are skipped");
        assert_eq!(at(1300.0, 950.0), None, "desktop");
        assert_eq!(at(799.999, 200.0), Some(12));
        assert_eq!(at(800.0, 200.0), Some(13), "right edge is exclusive");
    }

    #[test]
    fn appkit_coordinates_flip_around_the_main_display() {
        let main_height = 982.0;
        assert_eq!(
            point_from_appkit(10.0, 982.0, main_height),
            Point::new(10.0, 0.0)
        );
        assert_eq!(
            point_from_appkit(10.0, 0.0, main_height),
            Point::new(10.0, 982.0)
        );
        // A display above the main one has a negative y in global points.
        let above = rect(0.0, 982.0, 1920.0, 1080.0);
        assert_eq!(
            rect_from_appkit(&above, main_height),
            rect(0.0, -1080.0, 1920.0, 1080.0)
        );
        // An external display to the right, bottom-aligned with a taller main display.
        let right = rect(1512.0, 0.0, 800.0, 600.0);
        let global = rect_from_appkit(&right, main_height);
        assert_eq!(global, rect(1512.0, 382.0, 800.0, 600.0));
        assert_eq!(rect_to_appkit(&global, main_height), right);
    }

    #[test]
    fn display_under_point_across_displays() {
        let ds = displays();
        let at = |x, y| display_at(&ds, Point::new(x, y)).map(|d| d.id);
        assert_eq!(at(0.0, 0.0), Some(1));
        assert_eq!(at(1511.5, 500.0), Some(1));
        assert_eq!(at(1512.0, 500.0), Some(2));
        assert_eq!(at(2000.0, 1000.0), Some(2));
        assert_eq!(at(100.0, 1000.0), None, "below the shorter display");
    }

    #[test]
    fn regions_normalize_in_any_direction() {
        let d = &displays()[0];
        let expected = rect(100.0, 50.0, 200.0, 150.0);
        let a = Point::new(100.0, 50.0);
        let b = Point::new(300.0, 200.0);
        assert_eq!(region_between(d, a, b), expected);
        assert_eq!(region_between(d, b, a), expected);
        assert_eq!(
            region_between(d, Point::new(300.0, 50.0), Point::new(100.0, 200.0)),
            expected
        );
        assert_eq!(
            region_between(d, Point::new(100.0, 200.0), Point::new(300.0, 50.0)),
            expected
        );
    }

    #[test]
    fn regions_clamp_to_their_display_and_snap_to_pixels() {
        let ds = displays();
        let retina = &ds[0];
        let r = region_between(
            retina,
            Point::new(1400.0, 900.0),
            Point::new(1700.0, 1100.0),
        );
        assert_eq!(r, rect(1400.0, 900.0, 112.0, 82.0));
        let r = region_between(retina, Point::new(-30.0, -5.0), Point::new(10.3, 20.2));
        assert_eq!(r, rect(0.0, 0.0, 10.5, 20.0));

        let external = &ds[1];
        let r = region_between(external, Point::new(1600.4, 10.6), Point::new(1500.0, 40.2));
        assert_eq!(r, rect(0.0, 11.0, 88.0, 29.0), "relative to its display");
    }

    #[test]
    fn points_to_pixels_on_retina() {
        let r = rect(10.5, 20.0, 100.25, 50.0);
        assert_eq!(
            to_pixels(&r, 2.0),
            PixelRect {
                x: 21,
                y: 40,
                width: 201,
                height: 100
            }
        );
        assert_eq!(
            to_pixels(&rect(0.0, 0.0, 1512.0, 982.0), 2.0),
            PixelRect {
                x: 0,
                y: 0,
                width: 3024,
                height: 1964
            }
        );
        assert_eq!(
            to_pixels(&rect(3.0, 4.0, 5.0, 6.0), 1.0),
            PixelRect {
                x: 3,
                y: 4,
                width: 5,
                height: 6
            }
        );
    }

    #[test]
    fn click_picks_the_highlighted_window() {
        let mut picker = Picker::new(PickMode::Window, displays(), windows(), OWN_PID);
        picker.move_to(Point::new(300.0, 200.0));
        assert_eq!(
            picker.highlight(),
            Highlight::Window {
                bounds: rect(200.0, 150.0, 600.0, 400.0),
                label: "App 12  600 × 400".into()
            }
        );
        picker.press(Point::new(300.0, 200.0));
        picker.drag_to(Point::new(301.0, 201.0));
        let Outcome::Picked(picked) = picker.release(Point::new(301.0, 201.0)) else {
            panic!("a small wiggle is still a click");
        };
        assert_eq!(picked.target, CaptureTarget::Window { window_id: 12 });
        assert_eq!(picked.display_id, 1);

        picker.press(Point::new(1300.0, 950.0));
        assert_eq!(picker.release(Point::new(1300.0, 950.0)), Outcome::Pending);
    }

    #[test]
    fn drag_selects_a_region_on_the_display_where_it_started() {
        let mut picker = Picker::new(PickMode::Area, displays(), windows(), OWN_PID);
        picker.move_to(Point::new(300.0, 200.0));
        assert_eq!(picker.highlight(), Highlight::None);
        picker.press(Point::new(1400.0, 100.0));
        picker.drag_to(Point::new(1450.0, 150.0));
        assert!(picker.is_dragging());
        assert_eq!(
            picker.highlight(),
            Highlight::Region {
                bounds: rect(1400.0, 100.0, 50.0, 50.0),
                label: "50 × 50".into()
            }
        );
        let Outcome::Picked(picked) = picker.release(Point::new(1700.0, 300.0)) else {
            panic!("expected a region");
        };
        assert_eq!(
            picked.target,
            CaptureTarget::Region {
                display_id: 1,
                rect: rect(1400.0, 100.0, 112.0, 200.0)
            }
        );
        assert_eq!(picked.bounds, rect(1400.0, 100.0, 112.0, 200.0));
        assert_eq!(picked.label, "112 × 200");
    }

    #[test]
    fn region_on_a_second_display_is_relative_to_it() {
        let mut picker = Picker::new(PickMode::Area, displays(), windows(), OWN_PID);
        picker.press(Point::new(1612.0, 50.0));
        let Outcome::Picked(picked) = picker.release(Point::new(1812.0, 150.0)) else {
            panic!("expected a region");
        };
        assert_eq!(
            picked.target,
            CaptureTarget::Region {
                display_id: 2,
                rect: rect(100.0, 50.0, 200.0, 100.0)
            }
        );
        assert_eq!(picked.bounds, rect(1612.0, 50.0, 200.0, 100.0));
    }

    #[test]
    fn area_mode_only_drags() {
        let mut picker = Picker::new(PickMode::Area, displays(), windows(), OWN_PID);
        picker.move_to(Point::new(300.0, 200.0));
        assert_eq!(picker.highlight(), Highlight::None, "no window highlight");
        picker.press(Point::new(300.0, 200.0));
        assert_eq!(
            picker.release(Point::new(300.0, 200.0)),
            Outcome::Pending,
            "a click over a window picks nothing"
        );
        assert_eq!(picker.mode(), PickMode::Area);
    }

    #[test]
    fn window_mode_only_picks_windows() {
        let mut picker = Picker::new(PickMode::Window, displays(), windows(), OWN_PID);
        picker.move_to(Point::new(1300.0, 950.0));
        assert_eq!(picker.highlight(), Highlight::None, "desktop");
        picker.press(Point::new(1300.0, 950.0));
        assert_eq!(picker.release(Point::new(1300.0, 950.0)), Outcome::Pending);

        picker.press(Point::new(250.0, 200.0));
        picker.drag_to(Point::new(450.0, 300.0));
        assert!(!picker.is_dragging());
        assert!(
            !matches!(picker.highlight(), Highlight::Region { .. }),
            "no selection"
        );
        let Outcome::Picked(picked) = picker.release(Point::new(450.0, 300.0)) else {
            panic!("the window under the pointer");
        };
        assert_eq!(picked.target, CaptureTarget::Window { window_id: 12 });
    }

    #[test]
    fn display_mode_only_picks_displays() {
        let mut picker = Picker::new(PickMode::Display, displays(), windows(), OWN_PID);
        picker.move_to(Point::new(2000.0, 500.0));
        assert_eq!(
            picker.highlight(),
            Highlight::Display {
                bounds: rect(1512.0, 0.0, 1920.0, 1080.0),
                label: "External  1920 × 1080".into()
            }
        );
        picker.move_to(Point::new(300.0, 200.0));
        assert_eq!(
            picker.highlight(),
            Highlight::Display {
                bounds: rect(0.0, 0.0, 1512.0, 982.0),
                label: "Built-in  1512 × 982".into()
            },
            "follows the pointer, ignoring windows"
        );
        picker.press(Point::new(2000.0, 500.0));
        picker.drag_to(Point::new(2200.0, 600.0));
        let Outcome::Picked(picked) = picker.release(Point::new(2200.0, 600.0)) else {
            panic!("expected the display");
        };
        assert_eq!(picked.target, CaptureTarget::Display { display_id: 2 });
        assert_eq!(picked.display_id, 2);

        picker.press(Point::new(100.0, 1000.0));
        assert_eq!(
            picker.release(Point::new(100.0, 1000.0)),
            Outcome::Pending,
            "between displays"
        );
    }

    #[test]
    fn each_mode_keeps_its_cursor_on_every_display() {
        for (mode, expected) in [
            (PickMode::Area, PickerCursor::Crosshair),
            (PickMode::Window, PickerCursor::Camera),
            (PickMode::Display, PickerCursor::Camera),
        ] {
            let mut picker = Picker::new(mode, displays(), windows(), OWN_PID);
            for p in [Point::new(300.0, 200.0), Point::new(2000.0, 500.0)] {
                picker.move_to(p);
                assert_eq!(picker.cursor(), expected, "{mode:?} {p:?}");
                picker.press(p);
                picker.drag_to(Point::new(p.x + 100.0, p.y + 60.0));
                assert_eq!(picker.cursor(), expected, "{mode:?} while dragging");
                picker.release(Point::new(p.x + 100.0, p.y + 60.0));
            }
            if mode != PickMode::Area {
                assert_eq!(picker.pointer_label(), None);
            }
        }
    }

    #[test]
    fn the_label_shows_display_coordinates_then_the_selection_size() {
        let mut picker = Picker::new(PickMode::Area, displays(), windows(), OWN_PID);
        assert_eq!(picker.pointer_label(), None, "pointer unknown");
        picker.move_to(Point::new(100.6, 50.2));
        assert_eq!(
            picker.pointer_label(),
            Some(PointerLabel {
                display: 0,
                at: Point::new(100.6, 50.2),
                text: "100, 50".into()
            })
        );
        picker.move_to(Point::new(1612.0, 40.0));
        let label = picker.pointer_label().unwrap();
        assert_eq!((label.display, label.text.as_str()), (1, "100, 40"));

        picker.press(Point::new(1612.0, 40.0));
        picker.drag_to(Point::new(1712.0, 90.0));
        let label = picker.pointer_label().unwrap();
        assert_eq!(label.display, 1);
        assert_eq!(label.at, Point::new(200.0, 90.0));
        assert_eq!(label.text, "100 × 50");

        picker.drag_to(Point::new(1000.0, 90.0));
        let label = picker.pointer_label().unwrap();
        assert_eq!(label.display, 1, "stays on the display the drag started on");
        assert_eq!(label.at, Point::new(0.0, 90.0));
        assert_eq!(label.text, "100 × 50");

        picker.move_to(Point::new(100.0, 1000.0));
        picker.release(Point::new(100.0, 1000.0));
        assert_eq!(picker.pointer_label(), None, "between displays");
    }

    #[test]
    fn displays_are_found_by_index() {
        let picker = Picker::new(PickMode::Area, displays(), windows(), OWN_PID);
        assert_eq!(picker.display_index_at(Point::new(10.0, 10.0)), Some(0));
        assert_eq!(picker.display_index_at(Point::new(1600.0, 1000.0)), Some(1));
        assert_eq!(picker.display_index_at(Point::new(100.0, 1000.0)), None);
    }

    #[test]
    fn zero_sized_drags_keep_the_picker_open() {
        let mut picker = Picker::new(PickMode::Area, displays(), windows(), OWN_PID);
        picker.press(Point::new(1511.0, 10.0));
        assert_eq!(picker.release(Point::new(1700.0, 10.0)), Outcome::Pending);
    }
}
