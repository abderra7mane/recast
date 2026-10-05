//! Size, placement, buttons and auto-dismiss timing of the screenshot thumbnail.

use std::time::{Duration, Instant};

use recast_project::Rect;

const MAX_WIDTH: f64 = 240.0;
const MAX_HEIGHT: f64 = 160.0;
const MIN_WIDTH: f64 = 150.0;
const MIN_HEIGHT: f64 = 110.0;
/// Distance from the display's visible edges.
pub const MARGIN: f64 = 20.0;
pub const DISMISS_AFTER: Duration = Duration::from_secs(6);

const PILL_WIDTH: f64 = 92.0;
const PILL_HEIGHT: f64 = 24.0;
const PILL_GAP: f64 = 6.0;
const CLOSE_SIZE: f64 = 20.0;
const CLOSE_INSET: f64 = 6.0;

/// Card size in points for an image of `width × height` points: the image scaled to
/// fit the largest card, with room left for the buttons.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Close,
    Copy,
    Save,
    Beautify,
}

impl Button {
    pub fn label(self) -> &'static str {
        match self {
            Self::Close => "✕",
            Self::Copy => "Copy",
            Self::Save => "Save",
            Self::Beautify => "Beautify",
        }
    }
}

/// Buttons and their rectangles in the card, top-left origin.
pub fn buttons(card: (f64, f64)) -> [(Button, Rect); 4] {
    let stack = 3.0 * PILL_HEIGHT + 2.0 * PILL_GAP;
    let top = (card.1 - stack) / 2.0;
    let pill = |row: f64| Rect {
        x: (card.0 - PILL_WIDTH) / 2.0,
        y: top + row * (PILL_HEIGHT + PILL_GAP),
        width: PILL_WIDTH,
        height: PILL_HEIGHT,
    };
    [
        (
            Button::Close,
            Rect {
                x: CLOSE_INSET,
                y: CLOSE_INSET,
                width: CLOSE_SIZE,
                height: CLOSE_SIZE,
            },
        ),
        (Button::Copy, pill(0.0)),
        (Button::Save, pill(1.0)),
        (Button::Beautify, pill(2.0)),
    ]
}

pub fn button_at(card: (f64, f64), x: f64, y: f64) -> Option<Button> {
    buttons(card)
        .into_iter()
        .find(|(_, r)| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
        .map(|(button, _)| button)
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
    fn card_fits_the_image_with_room_for_buttons() {
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
    fn buttons_are_hit_tested_by_their_rectangles() {
        let card = (240.0, 156.0);
        assert_eq!(button_at(card, 10.0, 10.0), Some(Button::Close));
        assert_eq!(button_at(card, 120.0, 78.0), Some(Button::Save));
        let [_, (_, copy), _, (_, beautify)] = buttons(card);
        assert_eq!(button_at(card, 120.0, copy.y + 1.0), Some(Button::Copy));
        assert_eq!(
            button_at(card, 120.0, beautify.y + beautify.height - 1.0),
            Some(Button::Beautify)
        );
        assert_eq!(button_at(card, 120.0, copy.y - 1.0), None);
        assert_eq!(button_at(card, 230.0, 140.0), None);
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
