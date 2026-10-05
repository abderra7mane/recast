use recast_project::{
    EditSettings, EventKind, EventLog, InputEvent, MouseButton, Rect, ZoomFocus, ZoomSegment,
};

use super::*;

const BOUNDS: Rect = Rect {
    x: 100.0,
    y: 50.0,
    width: 1000.0,
    height: 500.0,
};

fn at(x: f64, y: f64) -> (f64, f64) {
    (BOUNDS.x + x * BOUNDS.width, BOUNDS.y + y * BOUNDS.height)
}

fn moved(t_ms: f64, x: f64, y: f64) -> InputEvent {
    let (x, y) = at(x, y);
    InputEvent {
        t_ms,
        kind: EventKind::Move { x, y },
    }
}

fn click(t_ms: f64, x: f64, y: f64) -> [InputEvent; 2] {
    let (x, y) = at(x, y);
    [
        InputEvent {
            t_ms,
            kind: EventKind::Down {
                x,
                y,
                button: MouseButton::Left,
                click_count: 1,
            },
        },
        InputEvent {
            t_ms: t_ms + 80.0,
            kind: EventKind::Up {
                x,
                y,
                button: MouseButton::Left,
            },
        },
    ]
}

fn log(mut events: Vec<InputEvent>) -> EventLog {
    events.sort_by(|a, b| a.t_ms.total_cmp(&b.t_ms));
    EventLog {
        events,
        ..EventLog::default()
    }
}

fn no_smoothing() -> EditSettings {
    let mut s = EditSettings::default();
    s.cursor.smoothing = 0.0;
    s
}

/// One click at (0.3, 0.4) at 2 s, cursor parked there from 1 s.
fn single_click() -> EventLog {
    let mut events = vec![moved(0.0, 0.1, 0.1), moved(1_000.0, 0.3, 0.4)];
    events.extend(click(2_000.0, 0.3, 0.4));
    log(events)
}

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

#[test]
fn camera_zooms_in_and_out_around_a_click() {
    let timeline = Timeline::new(&single_click(), &BOUNDS, &no_smoothing(), 10_000.0);
    assert_eq!(timeline.segments().len(), 1);
    let segment = &timeline.segments()[0];
    assert_eq!((segment.start_ms, segment.end_ms), (1_500.0, 3_500.0));

    assert_eq!(timeline.camera(1_000.0), Camera::FULL);
    let at_click = timeline.camera(2_000.0);
    assert!(at_click.scale > 1.7 && at_click.scale < 2.0, "{at_click:?}");
    let settled = timeline.camera(3_400.0);
    assert!(close(settled.scale, 2.0, 0.01), "{settled:?}");
    assert!(close(settled.center.x, 0.3, 0.01) && close(settled.center.y, 0.4, 0.01));
    let after = timeline.camera(6_000.0);
    assert!(close(after.scale, 1.0, 0.01), "{after:?}");
}

#[test]
fn zoom_eases_without_overshoot() {
    let timeline = Timeline::new(&single_click(), &BOUNDS, &no_smoothing(), 10_000.0);
    let mut previous = 1.0;
    for i in 0..400 {
        let t = 1_500.0 + i as f64 * 5.0;
        let scale = timeline.camera(t).scale;
        assert!(
            scale >= previous - 1e-9 && scale <= 2.0 + 1e-9,
            "{t}: {scale}"
        );
        previous = scale;
    }
    let mut previous = timeline.camera(3_500.0).scale;
    for i in 0..=400 {
        let scale = timeline.camera(3_500.0 + i as f64 * 5.0).scale;
        assert!(scale <= previous + 1e-9 && scale >= 1.0);
        previous = scale;
    }
}

#[test]
fn view_never_leaves_the_screen() {
    let mut events = vec![moved(0.0, 0.0, 0.0)];
    events.extend(click(1_000.0, 0.01, 0.02));
    events.extend(click(5_000.0, 0.99, 0.98));
    events.push(moved(5_500.0, 1.0, 1.0));
    let mut settings = no_smoothing();
    settings.zoom.level = 3.0;
    let timeline = Timeline::new(&log(events), &BOUNDS, &settings, 9_000.0);
    for i in 0..=1_800 {
        let camera = timeline.camera(i as f64 * 5.0);
        let (x, y, w, h) = camera.view();
        assert!(x >= -1e-9 && y >= -1e-9, "{camera:?}");
        assert!(x + w <= 1.0 + 1e-9 && y + h <= 1.0 + 1e-9, "{camera:?}");
    }
    let corner = timeline.camera(2_000.0);
    assert!(close(corner.view().0, 0.0, 0.01) && close(corner.view().1, 0.0, 0.01));
}

#[test]
fn following_respects_the_dead_zone() {
    let mut events = vec![moved(0.0, 0.5, 0.5)];
    events.extend(click(1_000.0, 0.5, 0.5));
    for i in 0..=10 {
        events.push(moved(1_500.0 + i as f64 * 10.0, 0.5 + 0.01 * i as f64, 0.5));
    }
    events.extend(click(2_200.0, 0.6, 0.5));
    let timeline = Timeline::new(&log(events), &BOUNDS, &no_smoothing(), 4_000.0);
    let camera = timeline.camera(2_400.0);
    assert!(close(camera.center.x, 0.5, 1e-3), "{camera:?}");

    let mut events = vec![moved(0.0, 0.5, 0.5)];
    events.extend(click(1_000.0, 0.5, 0.5));
    for i in 0..=25 {
        events.push(moved(1_500.0 + i as f64 * 10.0, 0.5 + 0.01 * i as f64, 0.5));
    }
    events.extend(click(2_200.0, 0.75, 0.5));
    let timeline = Timeline::new(&log(events), &BOUNDS, &no_smoothing(), 4_000.0);
    let camera = timeline.camera(3_000.0);
    assert!(close(camera.center.x, 0.625, 0.01), "{camera:?}");
    assert!(close(camera.center.y, 0.5, 1e-3));
}

#[test]
fn manual_segments_replace_auto_zoom() {
    let mut settings = no_smoothing();
    settings.zoom.auto = false;
    settings.zoom.segments = vec![ZoomSegment {
        start_ms: 4_000.0,
        end_ms: 6_000.0,
        level: 3.0,
        focus: ZoomFocus::Point { x: 0.8, y: 0.2 },
    }];
    let timeline = Timeline::new(&single_click(), &BOUNDS, &settings, 10_000.0);
    assert_eq!(timeline.camera(2_500.0), Camera::FULL);
    let camera = timeline.camera(5_900.0);
    assert!(close(camera.scale, 3.0, 0.02), "{camera:?}");
    assert!(close(camera.center.x, 0.8, 0.01) && close(camera.center.y, 0.2, 0.01));

    settings.zoom.segments.clear();
    let timeline = Timeline::new(&single_click(), &BOUNDS, &settings, 10_000.0);
    assert!(timeline.segments().is_empty());
    assert_eq!(timeline.camera(2_500.0), Camera::FULL);
}

#[test]
fn camera_is_a_pure_function_of_time() {
    let a = Timeline::new(&single_click(), &BOUNDS, &EditSettings::default(), 10_000.0);
    let b = Timeline::new(&single_click(), &BOUNDS, &EditSettings::default(), 10_000.0);
    let times = [2_345.6, 17.0, 9_999.0, 2_345.6, 0.0];
    let first: Vec<Camera> = times.iter().map(|t| a.camera(*t)).collect();
    let again: Vec<Camera> = times.iter().rev().map(|t| b.camera(*t)).collect();
    assert_eq!(first, again.into_iter().rev().collect::<Vec<_>>());
    assert_eq!(a.cursor(1_234.5), b.cursor(1_234.5));
}

#[test]
fn cursor_without_smoothing_follows_the_recording() {
    let timeline = Timeline::new(&single_click(), &BOUNDS, &no_smoothing(), 10_000.0);
    let cursor = timeline.cursor(500.0).unwrap();
    assert!(close(cursor.pos.x, 0.1, 1e-9) && close(cursor.pos.y, 0.1, 1e-9));
    let halfway = timeline.cursor(975.0).unwrap();
    assert!(close(halfway.pos.x, 0.2, 1e-9), "{halfway:?}");
    assert!(close(timeline.cursor(1_010.0).unwrap().pos.x, 0.3, 1e-9));
}

#[test]
fn smoothing_lags_then_settles() {
    let events = log(vec![moved(0.0, 0.0, 0.5), moved(1_000.0, 1.0, 0.5)]);
    let mut previous_lag = 0.0;
    for strength in [0.0, 0.3, 0.7, 1.0] {
        let mut settings = EditSettings::default();
        settings.cursor.smoothing = strength;
        let timeline = Timeline::new(&events, &BOUNDS, &settings, 5_000.0);
        let x = timeline.cursor(1_050.0).unwrap().pos.x;
        let lag = 1.0 - x;
        assert!(lag >= previous_lag, "strength {strength}: lag {lag}");
        previous_lag = lag;
        assert!(close(timeline.cursor(4_000.0).unwrap().pos.x, 1.0, 1e-3));
        let mut last = 0.0;
        for i in 0..400 {
            let x = timeline.cursor(900.0 + i as f64 * 5.0).unwrap().pos.x;
            assert!(x >= last - 1e-9 && x <= 1.0 + 1e-9);
            last = x;
        }
    }
    assert!(previous_lag > 0.5);
}

#[test]
fn no_events_means_no_cursor_and_no_zoom() {
    let timeline = Timeline::new(
        &EventLog::default(),
        &BOUNDS,
        &EditSettings::default(),
        3_000.0,
    );
    assert!(timeline.cursor(1_000.0).is_none());
    assert_eq!(timeline.camera(1_000.0), Camera::FULL);
    assert_eq!(timeline.ripples(1_000.0).count(), 0);
}

#[test]
fn ripples_expand_and_end() {
    let timeline = Timeline::new(&single_click(), &BOUNDS, &no_smoothing(), 10_000.0);
    assert_eq!(timeline.ripples(1_999.0).count(), 0);
    let ripple = timeline.ripples(2_110.0).next().unwrap();
    assert!(close(ripple.progress, 110.0 / RIPPLE_MS, 1e-9));
    assert!(close(ripple.pos.x, 0.3, 1e-9) && close(ripple.pos.y, 0.4, 1e-9));
    assert_eq!(timeline.ripples(2_000.0 + RIPPLE_MS).count(), 0);
}

#[test]
fn press_rises_and_releases() {
    let timeline = Timeline::new(&single_click(), &BOUNDS, &no_smoothing(), 10_000.0);
    let press = |t: f64| timeline.cursor(t).unwrap().press;
    assert_eq!(press(1_990.0), 0.0);
    assert!(press(2_030.0) > 0.3 && press(2_030.0) < 1.0);
    assert_eq!(press(2_070.0), 1.0);
    assert!(press(2_150.0) > 0.0 && press(2_150.0) < 1.0);
    assert_eq!(press(2_400.0), 0.0);
}

#[test]
fn idle_cursor_fades_when_enabled() {
    let mut settings = no_smoothing();
    let timeline = Timeline::new(&single_click(), &BOUNDS, &settings, 10_000.0);
    assert_eq!(timeline.cursor(8_000.0).unwrap().opacity, 1.0);

    settings.cursor.hide_when_idle = true;
    let mut events = single_click().events;
    events.push(moved(6_000.0, 0.5, 0.5));
    let timeline = Timeline::new(&log(events), &BOUNDS, &settings, 10_000.0);
    let opacity = |t: f64| timeline.cursor(t).unwrap().opacity;
    assert_eq!(opacity(2_100.0), 1.0);
    assert_eq!(opacity(2_080.0 + CURSOR_IDLE_MS), 1.0);
    assert_eq!(opacity(2_080.0 + CURSOR_IDLE_MS + 300.0), 0.0);
    let fading_in = opacity(5_900.0);
    assert!(fading_in > 0.0 && fading_in < 1.0, "{fading_in}");
    assert_eq!(opacity(6_000.0), 1.0);
}

#[test]
fn cursor_shape_follows_shape_events() {
    let mut events = single_click().events;
    events.insert(
        0,
        InputEvent {
            t_ms: 0.0,
            kind: EventKind::Cursor { shape: 3 },
        },
    );
    events.push(InputEvent {
        t_ms: 2_500.0,
        kind: EventKind::Cursor { shape: 5 },
    });
    let timeline = Timeline::new(&log(events), &BOUNDS, &no_smoothing(), 10_000.0);
    assert_eq!(timeline.cursor(100.0).unwrap().shape, Some(3));
    assert_eq!(timeline.cursor(2_600.0).unwrap().shape, Some(5));
}

#[test]
fn clicks_in_other_windows_do_not_zoom_or_ripple() {
    let window = Rect {
        x: 100.0,
        y: 100.0,
        width: 800.0,
        height: 600.0,
    };
    let mut events = vec![InputEvent {
        t_ms: 0.0,
        kind: EventKind::Move {
            x: 1_200.0,
            y: 400.0,
        },
    }];
    for (t, down) in [(2_000.0, true), (2_080.0, false)] {
        let (x, y, button) = (1_200.0, 400.0, MouseButton::Left);
        events.push(InputEvent {
            t_ms: t,
            kind: if down {
                EventKind::Down {
                    x,
                    y,
                    button,
                    click_count: 1,
                }
            } else {
                EventKind::Up { x, y, button }
            },
        });
    }
    let timeline = Timeline::new(&log(events), &window, &no_smoothing(), 5_000.0);
    assert!(timeline.segments().is_empty());
    assert_eq!(timeline.camera(2_500.0), Camera::FULL);
    assert_eq!(timeline.ripples(2_100.0).count(), 0);
    assert!(!timeline.clicks()[0].on_screen());
}
