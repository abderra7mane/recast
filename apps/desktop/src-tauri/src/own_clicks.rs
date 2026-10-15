//! Keeps clicks on Recast's own recording controls (the control bar, the menu bar
//! icon and its menu) out of recordings, so they get no click sound, ripple or zoom.

use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use recast_project::{EventKind, MouseButton, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    ControlBar,
    MenuBarIcon,
}

/// Global display points, origin at the top-left of the main display, like input events.
static AREAS: Mutex<[Option<Rect>; 2]> = Mutex::new([None, None]);

/// Where `area` is on screen, or `None` when it is hidden.
pub fn set(area: Area, rect: Option<Rect>) {
    AREAS.lock().unwrap_or_else(|e| e.into_inner())[area as usize] = rect;
}

static MENU_OPEN: AtomicBool = AtomicBool::new(false);

/// While the menu bar icon's menu is open, every click goes to it.
pub fn set_menu_open(open: bool) {
    MENU_OPEN.store(open, Ordering::Release);
}

fn on_own_ui(x: f64, y: f64) -> bool {
    if MENU_OPEN.load(Ordering::Acquire) {
        return true;
    }
    AREAS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .flatten()
        .any(|r| contains(r, x, y))
}

fn contains(r: &Rect, x: f64, y: f64) -> bool {
    x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}

fn index(button: MouseButton) -> usize {
    match button {
        MouseButton::Left => 0,
        MouseButton::Right => 1,
        MouseButton::Other => 2,
    }
}

/// Drops presses that start on Recast's controls, with their release; their drags
/// become plain moves.
#[derive(Debug, Default)]
pub struct ClickFilter {
    held: [bool; 3],
}

impl ClickFilter {
    pub fn apply(&mut self, kind: EventKind) -> Option<EventKind> {
        self.apply_with(kind, on_own_ui)
    }

    fn apply_with(
        &mut self,
        kind: EventKind,
        on_own_ui: impl Fn(f64, f64) -> bool,
    ) -> Option<EventKind> {
        match kind {
            EventKind::Down { x, y, button, .. } => {
                let own = on_own_ui(x, y);
                self.held[index(button)] = own;
                (!own).then_some(kind)
            }
            EventKind::Drag { x, y, button } if self.held[index(button)] => {
                Some(EventKind::Move { x, y })
            }
            EventKind::Up { button, .. } if self.held[index(button)] => {
                self.held[index(button)] = false;
                None
            }
            kind => Some(kind),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn down(x: f64, button: MouseButton) -> EventKind {
        EventKind::Down {
            x,
            y: 5.0,
            button,
            click_count: 1,
        }
    }

    fn up(x: f64, button: MouseButton) -> EventKind {
        EventKind::Up { x, y: 5.0, button }
    }

    #[test]
    fn drops_presses_that_start_on_own_ui() {
        let own = |x: f64, _: f64| x < 10.0;
        let mut filter = ClickFilter::default();
        assert_eq!(filter.apply_with(down(1.0, MouseButton::Left), own), None);
        assert_eq!(
            filter.apply_with(
                EventKind::Drag {
                    x: 50.0,
                    y: 5.0,
                    button: MouseButton::Left
                },
                own
            ),
            Some(EventKind::Move { x: 50.0, y: 5.0 })
        );
        assert_eq!(
            filter.apply_with(up(50.0, MouseButton::Left), own),
            None,
            "released outside, still dropped"
        );
        assert_eq!(
            filter.apply_with(down(50.0, MouseButton::Left), own),
            Some(down(50.0, MouseButton::Left))
        );
        assert_eq!(
            filter.apply_with(up(1.0, MouseButton::Left), own),
            Some(up(1.0, MouseButton::Left)),
            "released on own ui after starting elsewhere"
        );
    }

    #[test]
    fn tracks_each_button_on_its_own() {
        let own = |x: f64, _: f64| x < 10.0;
        let mut filter = ClickFilter::default();
        assert_eq!(filter.apply_with(down(1.0, MouseButton::Right), own), None);
        assert_eq!(
            filter.apply_with(down(50.0, MouseButton::Left), own),
            Some(down(50.0, MouseButton::Left))
        );
        assert_eq!(
            filter.apply_with(up(50.0, MouseButton::Left), own),
            Some(up(50.0, MouseButton::Left))
        );
        assert_eq!(filter.apply_with(up(50.0, MouseButton::Right), own), None);
    }

    #[test]
    fn areas_use_half_open_bounds() {
        let r = Rect {
            x: 10.0,
            y: 0.0,
            width: 20.0,
            height: 24.0,
        };
        assert!(contains(&r, 10.0, 0.0));
        assert!(contains(&r, 29.9, 23.9));
        assert!(!contains(&r, 30.0, 10.0));
        assert!(!contains(&r, 15.0, 24.0));
    }
}
