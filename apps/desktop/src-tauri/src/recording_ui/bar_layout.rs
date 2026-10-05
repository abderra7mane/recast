//! Size, placement and buttons of the recording control bar, in points with y down.

use recast_project::Rect;

pub const WIDTH: f64 = 332.0;
pub const HEIGHT: f64 = 44.0;
/// Distance above the bottom of the display's visible area.
const BOTTOM_MARGIN: f64 = 28.0;
const BUTTON_HEIGHT: f64 = 28.0;
const BUTTON_GAP: f64 = 6.0;
const RIGHT_INSET: f64 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarButton {
    Stop,
    Restart,
    Cancel,
}

impl BarButton {
    pub fn label(self) -> &'static str {
        match self {
            Self::Stop => "Stop",
            Self::Restart => "Restart",
            Self::Cancel => "Cancel",
        }
    }

    fn width(self) -> f64 {
        match self {
            Self::Stop => 64.0,
            Self::Restart => 74.0,
            Self::Cancel => 68.0,
        }
    }
}

/// The buttons from left to right, right-aligned in the bar.
pub fn buttons() -> [(BarButton, Rect); 3] {
    let order = [BarButton::Stop, BarButton::Restart, BarButton::Cancel];
    let total: f64 = order.iter().map(|b| b.width()).sum::<f64>() + BUTTON_GAP * 2.0;
    let mut x = WIDTH - RIGHT_INSET - total;
    let y = (HEIGHT - BUTTON_HEIGHT) / 2.0;
    order.map(|button| {
        let rect = Rect {
            x,
            y,
            width: button.width(),
            height: BUTTON_HEIGHT,
        };
        x += button.width() + BUTTON_GAP;
        (button, rect)
    })
}

pub fn button_at(x: f64, y: f64) -> Option<BarButton> {
    buttons()
        .into_iter()
        .find(|(_, r)| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
        .map(|(button, _)| button)
}

/// Where the elapsed time is drawn: between the record dot and the buttons.
pub fn time_rect() -> Rect {
    let left = 34.0;
    Rect {
        x: left,
        y: 0.0,
        width: buttons()[0].1.x - left,
        height: HEIGHT,
    }
}

/// Bottom-left origin of the bar in AppKit coordinates (y up): centered near the
/// bottom of `visible`, the display's frame without the menu bar and Dock.
pub fn origin(visible: &Rect) -> (f64, f64) {
    (
        visible.x + (visible.width - WIDTH) / 2.0,
        visible.y + BOTTOM_MARGIN,
    )
}

/// `m:ss`, or `h:mm:ss` past an hour.
pub fn elapsed_text(ms: f64) -> String {
    let total = (ms.max(0.0) / 1000.0).floor() as u64;
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_fit_in_the_bar_without_overlapping() {
        let buttons = buttons();
        for (_, r) in &buttons {
            assert!(r.x >= 0.0 && r.x + r.width <= WIDTH);
            assert!(r.y >= 0.0 && r.y + r.height <= HEIGHT);
        }
        for pair in buttons.windows(2) {
            assert!(pair[0].1.x + pair[0].1.width < pair[1].1.x);
        }
        assert!(time_rect().width >= 60.0, "room for h:mm:ss");
    }

    #[test]
    fn hit_testing_finds_each_button() {
        for (button, r) in buttons() {
            assert_eq!(
                button_at(r.x + r.width / 2.0, r.y + r.height / 2.0),
                Some(button)
            );
        }
        assert_eq!(button_at(10.0, HEIGHT / 2.0), None, "the drag area");
        assert_eq!(button_at(WIDTH - 2.0, HEIGHT / 2.0), None);
    }

    #[test]
    fn sits_centered_above_the_dock() {
        let visible = Rect {
            x: 1440.0,
            y: 80.0,
            width: 1920.0,
            height: 1000.0,
        };
        let (x, y) = origin(&visible);
        assert_eq!(x, 1440.0 + (1920.0 - WIDTH) / 2.0);
        assert_eq!(y, 80.0 + BOTTOM_MARGIN);
    }

    #[test]
    fn elapsed_time_reads_like_a_clock() {
        assert_eq!(elapsed_text(0.0), "0:00");
        assert_eq!(elapsed_text(59_999.0), "0:59");
        assert_eq!(elapsed_text(61_000.0), "1:01");
        assert_eq!(elapsed_text(3_725_000.0), "1:02:05");
        assert_eq!(elapsed_text(-5.0), "0:00");
    }
}
