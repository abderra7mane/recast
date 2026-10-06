//! Size, placement, gestures, menu and auto-dismiss timing of the screenshot thumbnail.

use std::time::{Duration, Instant};

use recast_project::Rect;

const MAX_WIDTH: f64 = 240.0;
const MAX_HEIGHT: f64 = 160.0;
const MIN_WIDTH: f64 = 150.0;
const MIN_HEIGHT: f64 = 110.0;
/// Distance from the display's visible edges.
pub const MARGIN: f64 = 20.0;
pub const DISMISS_AFTER: Duration = Duration::from_secs(6);
pub const SLIDE_OUT: Duration = Duration::from_millis(250);
pub const SNAP_BACK: Duration = Duration::from_millis(150);

/// How far the pointer moves before a press becomes a gesture.
const GESTURE_THRESHOLD: f64 = 3.0;
/// How far right the card travels before letting go dismisses it.
pub const DISMISS_DISTANCE: f64 = 48.0;
/// How far above or below the card a swipe may stray before it turns into a file drag.
const SWIPE_SLACK: f64 = 24.0;

/// Card size in points for an image of `width × height` points: the image scaled to
/// fit the largest card, but never smaller than the smallest one.
pub fn card_size(width: f64, height: f64) -> (f64, f64) {
    let scale = (MAX_WIDTH / width.max(1.0)).min(MAX_HEIGHT / height.max(1.0));
    (
        (width * scale).clamp(MIN_WIDTH, MAX_WIDTH),
        (height * scale).clamp(MIN_HEIGHT, MAX_HEIGHT),
    )
}

/// The image's rectangle inside the card, aspect-fit and centered.
pub fn image_rect(card: (f64, f64), image: (f64, f64)) -> Rect {
    let scale = (card.0 / image.0.max(1.0)).min(card.1 / image.1.max(1.0));
    let (width, height) = (image.0 * scale, image.1 * scale);
    Rect {
        x: (card.0 - width) / 2.0,
        y: (card.1 - height) / 2.0,
        width,
        height,
    }
}

/// Bottom-left origin of the card in AppKit coordinates (y up), in the bottom-right
/// corner of `visible`, the display's frame without the menu bar and Dock.
pub fn card_origin(visible: &Rect, card: (f64, f64)) -> (f64, f64) {
    (
        visible.x + visible.width - card.0 - MARGIN,
        visible.y + MARGIN,
    )
}

/// Where the card ends up when it slides out: just past the right edge of `screen`,
/// the display's full frame, at the same height.
pub fn slide_out_frame(card: &Rect, screen: &Rect) -> Rect {
    Rect {
        x: screen.x + screen.width,
        ..*card
    }
}

/// The card's x and opacity at `progress` (0 to 1) of sliding from `start_x` to `end_x`,
/// the display's right edge. It speeds up as it goes and fades as it crosses the edge,
/// so it never shows on a display to the right.
pub fn slide_out_at(start_x: f64, end_x: f64, width: f64, progress: f64) -> (f64, f64) {
    let t = progress.clamp(0.0, 1.0);
    let x = start_x + (end_x - start_x) * t * t;
    let visible = ((end_x - x) / width.max(1.0)).clamp(0.0, 1.0);
    (x, visible)
}

/// The card's x at `progress` of easing back from `from_x` to its place at `home_x`.
pub fn snap_back_at(from_x: f64, home_x: f64, progress: f64) -> f64 {
    let t = progress.clamp(0.0, 1.0);
    let eased = 1.0 - (1.0 - t) * (1.0 - t);
    from_x + (home_x - from_x) * eased
}

/// What a press on the card turns into once the pointer moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    /// Still within the click threshold.
    Undecided,
    /// A mostly horizontal drag to the right: the card follows and may be dismissed.
    Swipe,
    /// Anything else drags the file out.
    DragOut,
}

/// Classifies a press that has moved by `dx, dy` points (y down).
pub fn classify(dx: f64, dy: f64) -> Gesture {
    if dx.hypot(dy) < GESTURE_THRESHOLD {
        Gesture::Undecided
    } else if dx > 0.0 && dy.abs() <= dx * 0.5 {
        Gesture::Swipe
    } else {
        Gesture::DragOut
    }
}

/// Whether a swipe with the pointer at `y` in card coordinates (y down) has left the
/// card's band and should become a file drag.
pub fn swipe_strays(y: f64, card_height: f64) -> bool {
    y < -SWIPE_SLACK || y > card_height + SWIPE_SLACK
}

/// How far the card follows a swipe that moved `dx` points; it never moves left.
pub fn swipe_offset(dx: f64) -> f64 {
    dx.max(0.0)
}

pub fn swipe_dismisses(offset: f64) -> bool {
    offset >= DISMISS_DISTANCE
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollPhase {
    Began,
    Changed,
    Ended,
    Cancelled,
    /// Momentum, mouse wheels and anything else a swipe ignores.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SwipeUpdate {
    Ignore,
    /// Move the card this far right of its place.
    Follow(f64),
    Dismiss,
    SnapBack,
}

/// Turns a two-finger trackpad swipe over the card into card moves. The card is
/// dismissed as soon as it has gone far enough, since the pointer stays put while the
/// card moves away from under it.
#[derive(Debug, Default, Clone, Copy)]
pub struct TrackpadSwipe {
    dx: f64,
    dy: f64,
    gesture: Option<Gesture>,
    dismissed: bool,
}

impl TrackpadSwipe {
    /// `dx, dy` is how far the fingers moved, right and down positive.
    pub fn scroll(&mut self, phase: ScrollPhase, dx: f64, dy: f64) -> SwipeUpdate {
        match phase {
            ScrollPhase::Began => {
                *self = Self::default();
                self.add(dx, dy)
            }
            ScrollPhase::Changed => self.add(dx, dy),
            ScrollPhase::Ended | ScrollPhase::Cancelled => {
                let swiping = self.gesture == Some(Gesture::Swipe) && !self.dismissed;
                *self = Self::default();
                if swiping {
                    SwipeUpdate::SnapBack
                } else {
                    SwipeUpdate::Ignore
                }
            }
            ScrollPhase::Other => SwipeUpdate::Ignore,
        }
    }

    fn add(&mut self, dx: f64, dy: f64) -> SwipeUpdate {
        if self.dismissed {
            return SwipeUpdate::Ignore;
        }
        self.dx += dx;
        self.dy += dy;
        if self.gesture.is_none() {
            match classify(self.dx, self.dy) {
                Gesture::Undecided => return SwipeUpdate::Ignore,
                decided => self.gesture = Some(decided),
            }
        }
        if self.gesture != Some(Gesture::Swipe) {
            return SwipeUpdate::Ignore;
        }
        let offset = swipe_offset(self.dx);
        if swipe_dismisses(offset) {
            self.dismissed = true;
            SwipeUpdate::Dismiss
        } else {
            SwipeUpdate::Follow(offset)
        }
    }
}

/// How far the fingers moved along an axis (right or down positive), from a scroll
/// event's delta. With natural scrolling (`inverted`) the content moves with the fingers.
pub fn finger_delta(scrolling_delta: f64, inverted: bool) -> f64 {
    if inverted {
        scrolling_delta
    } else {
        -scrolling_delta
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Copy,
    Save,
    ShowInFinder,
    Edit,
    Delete,
    Close,
}

impl MenuItem {
    /// The context menu, top to bottom; `None` is a separator.
    pub const MENU: [Option<MenuItem>; 8] = [
        Some(Self::Copy),
        Some(Self::Save),
        Some(Self::ShowInFinder),
        Some(Self::Edit),
        None,
        Some(Self::Delete),
        None,
        Some(Self::Close),
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Save => "Save",
            Self::ShowInFinder => "Show in Finder",
            Self::Edit => "Edit",
            Self::Delete => "Delete",
            Self::Close => "Close",
        }
    }

    pub fn tag(self) -> isize {
        self as isize + 1
    }

    pub fn from_tag(tag: isize) -> Option<Self> {
        Self::MENU.into_iter().flatten().find(|i| i.tag() == tag)
    }
}

/// Counts down to dismissing the thumbnail; the countdown stops while paused.
#[derive(Debug, Clone, Copy)]
pub struct DismissTimer {
    remaining: Duration,
    running_since: Option<Instant>,
}

impl DismissTimer {
    pub fn start(duration: Duration, now: Instant) -> Self {
        Self {
            remaining: duration,
            running_since: Some(now),
        }
    }

    pub fn pause(&mut self, now: Instant) {
        if let Some(since) = self.running_since.take() {
            self.remaining = self.remaining.saturating_sub(now.duration_since(since));
        }
    }

    pub fn resume(&mut self, now: Instant) {
        self.running_since.get_or_insert(now);
    }

    pub fn expired(&self, now: Instant) -> bool {
        match self.running_since {
            Some(since) => now.duration_since(since) >= self.remaining,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_fits_the_image() {
        assert_eq!(card_size(1512.0, 982.0).0, 240.0);
        assert!((card_size(1512.0, 982.0).1 - 155.87).abs() < 0.01);
        assert_eq!(card_size(400.0, 1600.0), (150.0, 160.0));
        assert_eq!(card_size(2000.0, 100.0), (240.0, 110.0));
        assert_eq!(card_size(50.0, 40.0), (200.0, 160.0));
    }

    #[test]
    fn image_is_letterboxed_in_the_card() {
        let r = image_rect((240.0, 110.0), (2000.0, 100.0));
        assert_eq!(r.width, 240.0);
        assert_eq!(r.height, 12.0);
        assert_eq!(r.y, 49.0);
    }

    #[test]
    fn card_sits_in_the_bottom_right_corner() {
        let visible = Rect {
            x: 1512.0,
            y: 80.0,
            width: 1920.0,
            height: 1000.0,
        };
        assert_eq!(card_origin(&visible, (240.0, 150.0)), (3172.0, 100.0));
    }

    #[test]
    fn slide_out_ends_just_past_the_right_edge() {
        let screen = Rect {
            x: 1512.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };
        let card = Rect {
            x: 3172.0,
            y: 100.0,
            width: 240.0,
            height: 150.0,
        };
        assert_eq!(slide_out_frame(&card, &screen), Rect { x: 3432.0, ..card });
        let swiped = Rect { x: 3230.0, ..card };
        assert_eq!(slide_out_frame(&swiped, &screen).x, 3432.0);
        let left = Rect {
            x: -1920.0,
            ..screen
        };
        assert_eq!(slide_out_frame(&card, &left).x, 0.0, "display left of main");
    }

    #[test]
    fn rightward_drags_swipe_and_others_drag_the_file_out() {
        assert_eq!(classify(2.0, 1.0), Gesture::Undecided);
        assert_eq!(classify(10.0, 0.0), Gesture::Swipe);
        assert_eq!(classify(10.0, 5.0), Gesture::Swipe);
        assert_eq!(classify(10.0, -5.0), Gesture::Swipe);
        assert_eq!(classify(10.0, 6.0), Gesture::DragOut, "too steep");
        assert_eq!(classify(-10.0, 0.0), Gesture::DragOut, "leftward");
        assert_eq!(classify(0.0, -10.0), Gesture::DragOut, "upward");
        assert_eq!(classify(0.0, 10.0), Gesture::DragOut, "downward");
    }

    #[test]
    fn a_swipe_that_leaves_the_card_band_becomes_a_drag() {
        let height = 150.0;
        assert!(!swipe_strays(75.0, height));
        assert!(!swipe_strays(-24.0, height));
        assert!(!swipe_strays(174.0, height));
        assert!(swipe_strays(-25.0, height));
        assert!(swipe_strays(175.0, height));
    }

    #[test]
    fn letting_go_far_enough_right_dismisses() {
        assert_eq!(swipe_offset(-20.0), 0.0);
        assert!(!swipe_dismisses(swipe_offset(47.0)));
        assert!(swipe_dismisses(swipe_offset(48.0)));
    }

    #[test]
    fn trackpad_swipes_follow_and_dismiss() {
        let mut swipe = TrackpadSwipe::default();
        assert_eq!(
            swipe.scroll(ScrollPhase::Began, 1.0, 0.0),
            SwipeUpdate::Ignore
        );
        assert_eq!(
            swipe.scroll(ScrollPhase::Changed, 9.0, 1.0),
            SwipeUpdate::Follow(10.0)
        );
        assert_eq!(
            swipe.scroll(ScrollPhase::Other, 30.0, 0.0),
            SwipeUpdate::Ignore,
            "momentum"
        );
        assert_eq!(
            swipe.scroll(ScrollPhase::Changed, 37.0, 0.0),
            SwipeUpdate::Follow(47.0)
        );
        assert_eq!(
            swipe.scroll(ScrollPhase::Changed, 1.0, 0.0),
            SwipeUpdate::Dismiss
        );
        assert_eq!(
            swipe.scroll(ScrollPhase::Changed, 10.0, 0.0),
            SwipeUpdate::Ignore
        );
        assert_eq!(
            swipe.scroll(ScrollPhase::Ended, 0.0, 0.0),
            SwipeUpdate::Ignore
        );

        swipe.scroll(ScrollPhase::Began, 0.0, 0.0);
        swipe.scroll(ScrollPhase::Changed, 20.0, 0.0);
        assert_eq!(
            swipe.scroll(ScrollPhase::Ended, 0.0, 0.0),
            SwipeUpdate::SnapBack,
            "too short"
        );

        swipe.scroll(ScrollPhase::Began, 0.0, 0.0);
        swipe.scroll(ScrollPhase::Changed, 30.0, 0.0);
        assert_eq!(
            swipe.scroll(ScrollPhase::Changed, -50.0, 0.0),
            SwipeUpdate::Follow(0.0),
            "never left of its place"
        );
        assert_eq!(
            swipe.scroll(ScrollPhase::Cancelled, 0.0, 0.0),
            SwipeUpdate::SnapBack
        );
    }

    #[test]
    fn slide_out_speeds_up_and_fades_past_the_edge() {
        let (start, end, width) = (3172.0, 3432.0, 240.0);
        assert_eq!(slide_out_at(start, end, width, 0.0), (start, 1.0));
        assert_eq!(
            slide_out_at(start, end, width, 0.2).1,
            1.0,
            "opaque while inside the display"
        );
        let (x, alpha) = slide_out_at(start, end, width, 0.5);
        assert_eq!(x, start + 65.0, "a quarter of the way at half time");
        assert_eq!(alpha, (end - x) / width, "the share still on the display");
        let (x, alpha) = slide_out_at(start, end, width, 0.9);
        assert!((x - (start + 260.0 * 0.81)).abs() < 1e-9);
        assert!((alpha - (end - x) / width).abs() < 1e-9);
        assert_eq!(slide_out_at(start, end, width, 1.0), (end, 0.0));
        assert_eq!(slide_out_at(start, end, width, 1.5), (end, 0.0));
    }

    #[test]
    fn snap_back_eases_home() {
        assert_eq!(snap_back_at(3200.0, 3172.0, 0.0), 3200.0);
        assert_eq!(snap_back_at(3200.0, 3172.0, 0.5), 3179.0);
        assert_eq!(snap_back_at(3200.0, 3172.0, 1.0), 3172.0);
    }

    #[test]
    fn vertical_and_leftward_trackpad_swipes_are_ignored() {
        let mut swipe = TrackpadSwipe::default();
        swipe.scroll(ScrollPhase::Began, 0.0, 0.0);
        assert_eq!(
            swipe.scroll(ScrollPhase::Changed, 2.0, 30.0),
            SwipeUpdate::Ignore
        );
        assert_eq!(
            swipe.scroll(ScrollPhase::Changed, 100.0, 0.0),
            SwipeUpdate::Ignore,
            "decided once per gesture"
        );
        assert_eq!(
            swipe.scroll(ScrollPhase::Ended, 0.0, 0.0),
            SwipeUpdate::Ignore
        );

        swipe.scroll(ScrollPhase::Began, -10.0, 0.0);
        assert_eq!(
            swipe.scroll(ScrollPhase::Ended, 0.0, 0.0),
            SwipeUpdate::Ignore
        );
    }

    #[test]
    fn natural_scrolling_follows_the_fingers() {
        assert_eq!(finger_delta(12.0, true), 12.0);
        assert_eq!(finger_delta(12.0, false), -12.0);
    }

    #[test]
    fn menu_items_round_trip_through_tags() {
        let items: Vec<&str> = MenuItem::MENU.iter().flatten().map(|i| i.title()).collect();
        assert_eq!(
            items,
            ["Copy", "Save", "Show in Finder", "Edit", "Delete", "Close"]
        );
        for item in MenuItem::MENU.into_iter().flatten() {
            assert_ne!(item.tag(), 0, "0 is the default tag");
            assert_eq!(MenuItem::from_tag(item.tag()), Some(item));
        }
        assert_eq!(MenuItem::from_tag(0), None);
    }

    #[test]
    fn timer_pauses_while_hovered() {
        let t0 = Instant::now();
        let s = Duration::from_secs;
        let mut timer = DismissTimer::start(s(6), t0);
        assert!(!timer.expired(t0 + s(5)));
        timer.pause(t0 + s(4));
        assert!(!timer.expired(t0 + s(60)));
        timer.resume(t0 + s(60));
        timer.resume(t0 + s(61));
        assert!(!timer.expired(t0 + s(61)));
        assert!(timer.expired(t0 + s(62)));
    }
}
