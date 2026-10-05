use recast_project::{ZoomFocus, ZoomSegment};

use crate::{Click, Point};

/// Clicks closer together than this (in time) can share a segment.
pub const GROUP_GAP_MS: f64 = 3_000.0;
/// Zooming starts this long before the first click, so the view is in place by then.
pub const LEAD_MS: f64 = 500.0;
/// A segment ends this long after its last click.
pub const IDLE_MS: f64 = 1_500.0;
/// Segments separated by less than this are joined, so the view pans instead of
/// briefly zooming out.
pub const MIN_GAP_MS: f64 = 1_000.0;
/// Clicks share a segment when they fit in this fraction of the zoomed view.
const GROUP_SPAN: f64 = 0.5;

struct Group {
    first_ms: f64,
    last_ms: f64,
    min: Point,
    max: Point,
}

impl Group {
    fn new(click: &Click) -> Self {
        Self {
            first_ms: click.t_ms,
            last_ms: click.t_ms,
            min: click.pos,
            max: click.pos,
        }
    }

    fn accepts(&self, click: &Click, level: f64) -> bool {
        let span = GROUP_SPAN / level;
        let width = self.max.x.max(click.pos.x) - self.min.x.min(click.pos.x);
        let height = self.max.y.max(click.pos.y) - self.min.y.min(click.pos.y);
        click.t_ms - self.last_ms < GROUP_GAP_MS && width <= span && height <= span
    }

    fn add(&mut self, click: &Click) {
        self.last_ms = click.t_ms;
        self.min = Point::new(self.min.x.min(click.pos.x), self.min.y.min(click.pos.y));
        self.max = Point::new(self.max.x.max(click.pos.x), self.max.y.max(click.pos.y));
    }
}

/// Zoom segments for the button presses in `clicks` (sorted by time): one per group
/// of clicks that are close in time and on screen. Each follows the cursor.
pub fn auto_segments(clicks: &[Click], level: f64, duration_ms: f64) -> Vec<ZoomSegment> {
    let mut groups: Vec<Group> = Vec::new();
    for click in clicks.iter().filter(|c| c.down && c.on_screen()) {
        if click.t_ms < 0.0 || click.t_ms > duration_ms {
            continue;
        }
        match groups.last_mut() {
            Some(group) if group.accepts(click, level) => group.add(click),
            _ => groups.push(Group::new(click)),
        }
    }

    let mut segments: Vec<ZoomSegment> = Vec::new();
    for group in groups {
        let start_ms = (group.first_ms - LEAD_MS).max(0.0);
        let end_ms = (group.last_ms + IDLE_MS).min(duration_ms);
        if let Some(previous) = segments.last_mut()
            && start_ms - previous.end_ms < MIN_GAP_MS
        {
            let joint = previous.end_ms.min(start_ms).max(previous.start_ms);
            previous.end_ms = joint;
            segments.push(segment(joint, end_ms, level));
            continue;
        }
        segments.push(segment(start_ms, end_ms, level));
    }
    segments.retain(|s| s.end_ms > s.start_ms);
    segments
}

fn segment(start_ms: f64, end_ms: f64, level: f64) -> ZoomSegment {
    ZoomSegment {
        start_ms,
        end_ms,
        level,
        focus: ZoomFocus::FollowCursor,
    }
}

#[cfg(test)]
mod tests {
    use recast_project::MouseButton;

    use super::*;

    fn click(t_ms: f64, x: f64, y: f64) -> Click {
        Click {
            t_ms,
            pos: Point::new(x, y),
            button: MouseButton::Left,
            down: true,
        }
    }

    fn ranges(segments: &[ZoomSegment]) -> Vec<(f64, f64)> {
        segments.iter().map(|s| (s.start_ms, s.end_ms)).collect()
    }

    #[test]
    fn nearby_clicks_form_one_segment() {
        let clicks = [
            click(2_000.0, 0.40, 0.40),
            click(4_000.0, 0.45, 0.42),
            click(6_500.0, 0.50, 0.38),
        ];
        let segments = auto_segments(&clicks, 2.0, 60_000.0);
        assert_eq!(ranges(&segments), vec![(1_500.0, 8_000.0)]);
        assert_eq!(segments[0].level, 2.0);
        assert_eq!(segments[0].focus, ZoomFocus::FollowCursor);
    }

    #[test]
    fn long_pause_splits_segments() {
        let clicks = [click(1_000.0, 0.4, 0.4), click(6_000.0, 0.4, 0.4)];
        let segments = auto_segments(&clicks, 2.0, 60_000.0);
        assert_eq!(
            ranges(&segments),
            vec![(500.0, 2_500.0), (5_500.0, 7_500.0)]
        );
    }

    #[test]
    fn distant_clicks_split_into_adjacent_segments() {
        let clicks = [click(1_000.0, 0.1, 0.1), click(2_000.0, 0.9, 0.9)];
        let segments = auto_segments(&clicks, 2.0, 60_000.0);
        assert_eq!(
            ranges(&segments),
            vec![(500.0, 1_500.0), (1_500.0, 3_500.0)]
        );
    }

    #[test]
    fn short_gaps_are_joined() {
        let clicks = [click(1_000.0, 0.1, 0.1), click(3_800.0, 0.9, 0.9)];
        let segments = auto_segments(&clicks, 2.0, 60_000.0);
        assert_eq!(
            ranges(&segments),
            vec![(500.0, 2_500.0), (2_500.0, 5_300.0)]
        );
    }

    #[test]
    fn higher_zoom_needs_closer_clicks() {
        let clicks = [click(1_000.0, 0.3, 0.3), click(1_500.0, 0.5, 0.3)];
        assert_eq!(auto_segments(&clicks, 2.0, 60_000.0).len(), 1);
        assert_eq!(auto_segments(&clicks, 4.0, 60_000.0).len(), 2);
    }

    #[test]
    fn segments_stay_inside_the_recording() {
        let clicks = [
            click(100.0, 0.5, 0.5),
            click(4_900.0, 0.5, 0.5),
            click(9_000.0, 0.5, 0.5),
        ];
        let segments = auto_segments(&clicks, 2.0, 5_000.0);
        assert_eq!(ranges(&segments), vec![(0.0, 1_600.0), (4_400.0, 5_000.0)]);
    }

    #[test]
    fn clicks_outside_the_recorded_area_are_ignored() {
        let clicks = [click(1_000.0, 1.375, 0.5), click(1_500.0, 0.5, -0.01)];
        assert!(auto_segments(&clicks, 2.0, 60_000.0).is_empty());
        let clicks = [click(1_000.0, 1.0, 0.0)];
        assert_eq!(auto_segments(&clicks, 2.0, 60_000.0).len(), 1);
    }

    #[test]
    fn releases_are_ignored() {
        let mut up = click(1_000.0, 0.5, 0.5);
        up.down = false;
        assert!(auto_segments(&[up], 2.0, 60_000.0).is_empty());
    }
}
