//! Renders a small fixture at fixed times and compares the frames with the PNGs in
//! `tests/goldens`. Run with `UPDATE_GOLDENS=1` (`make update-goldens`) to rewrite them.

use std::{collections::HashMap, path::PathBuf};

use recast_project::{
    BackgroundFill, Color, EditSettings, EventKind, EventLog, InputEvent, MouseButton, Rect,
};
use recast_render::{
    Compositor, CpuFrame, FrameSource, PixelFormat, Result, Scene, SceneParts,
    bitmap::{self, Rgba},
    cursor::CursorImage,
};

const SCREEN_W: u32 = 640;
const SCREEN_H: u32 = 400;
const OUT_W: u32 = 560;
const OUT_H: u32 = 370;

/// A desktop-like test pattern: a window with a title bar, buttons and lines of "text".
struct Pattern {
    pixels: Vec<u8>,
}

impl Pattern {
    fn new() -> Self {
        let mut pixels = Vec::with_capacity((SCREEN_W * SCREEN_H * 4) as usize);
        for y in 0..SCREEN_H {
            for x in 0..SCREEN_W {
                let [r, g, b] = Self::color(x, y);
                pixels.extend_from_slice(&[b, g, r, 255]);
            }
        }
        Self { pixels }
    }

    fn color(x: u32, y: u32) -> [u8; 3] {
        let in_window = (60..580).contains(&x) && (40..360).contains(&y);
        if !in_window {
            return [
                40 + (y * 60 / SCREEN_H) as u8,
                70,
                110 + (x * 80 / SCREEN_W) as u8,
            ];
        }
        if y < 64 {
            return if (76..88).contains(&x) && (46..58).contains(&y) {
                [236, 95, 87]
            } else {
                [225, 225, 230]
            };
        }
        let buttons = [
            (280..330, 180..220, [52, 120, 246]),
            (400..450, 240..280, [48, 180, 90]),
        ];
        for (bx, by, color) in buttons {
            if bx.contains(&x) && by.contains(&y) {
                return color;
            }
        }
        let line = (y - 64) % 18;
        let text = (80..540).contains(&x) && line < 6 && !(x / 7 + y / 18).is_multiple_of(9);
        if text { [60, 60, 66] } else { [250, 250, 252] }
    }
}

impl FrameSource for Pattern {
    fn frame_at(&mut self, _t_ms: f64) -> Result<CpuFrame<'_>> {
        Ok(CpuFrame {
            width: SCREEN_W,
            height: SCREEN_H,
            bytes_per_row: SCREEN_W as usize * 4,
            format: PixelFormat::Bgra8,
            data: &self.pixels,
            id: Some(0),
        })
    }
}

fn events() -> EventLog {
    let mut events = Vec::new();
    let mut path = |from: (f64, f64), to: (f64, f64), start: f64, end: f64| {
        let steps = ((end - start) / 16.0) as usize;
        for i in 0..=steps {
            let f = i as f64 / steps as f64;
            let eased = f * f * (3.0 - 2.0 * f);
            events.push(InputEvent {
                t_ms: start + (end - start) * f,
                kind: EventKind::Move {
                    x: from.0 + (to.0 - from.0) * eased,
                    y: from.1 + (to.1 - from.1) * eased,
                },
            });
        }
    };
    path((80.0, 60.0), (300.0, 200.0), 0.0, 1_000.0);
    path((300.0, 200.0), (420.0, 260.0), 1_600.0, 2_600.0);
    for (t, x, y) in [(1_500.0, 300.0, 200.0), (2_800.0, 420.0, 260.0)] {
        events.push(InputEvent {
            t_ms: t,
            kind: EventKind::Down {
                x,
                y,
                button: MouseButton::Left,
                click_count: 1,
            },
        });
        events.push(InputEvent {
            t_ms: t + 80.0,
            kind: EventKind::Up {
                x,
                y,
                button: MouseButton::Left,
            },
        });
    }
    events.sort_by(|a, b| a.t_ms.total_cmp(&b.t_ms));
    EventLog {
        events,
        ..EventLog::default()
    }
}

fn fixture(settings: EditSettings) -> Scene {
    fixture_with_cursor(settings, None)
}

const CROSSHAIR: u32 = 7;
const CROSSHAIR_PTS: f64 = 20.0;
const CROSSHAIR_SCALE: f64 = 2.0;

/// A recorded-style cursor at 2 pixels per point: 20 × 20 points with its hotspot in
/// the middle, a red dot (radius 3 pt) inside a blue ring (6–9 pt).
fn crosshair() -> CursorImage {
    let size = (CROSSHAIR_PTS * CROSSHAIR_SCALE) as u32;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f64 + 0.5) / CROSSHAIR_SCALE - CROSSHAIR_PTS / 2.0;
            let dy = (y as f64 + 0.5) / CROSSHAIR_SCALE - CROSSHAIR_PTS / 2.0;
            let r = (dx * dx + dy * dy).sqrt();
            pixels.extend_from_slice(&if r <= 3.0 {
                [230, 20, 20, 255]
            } else if (6.0..=9.0).contains(&r) {
                [20, 40, 230, 255]
            } else {
                [0, 0, 0, 0]
            });
        }
    }
    CursorImage {
        image: Rgba::from_straight(size, size, pixels),
        width_pts: CROSSHAIR_PTS,
        height_pts: CROSSHAIR_PTS,
        hotspot_x: CROSSHAIR_PTS / 2.0,
        hotspot_y: CROSSHAIR_PTS / 2.0,
    }
}

fn fixture_with_cursor(settings: EditSettings, cursor: Option<CursorImage>) -> Scene {
    let mut events = events();
    let mut cursors = HashMap::new();
    if let Some(cursor) = cursor {
        cursors.insert(CROSSHAIR, cursor);
        events.events.insert(
            0,
            InputEvent {
                t_ms: 0.0,
                kind: EventKind::Cursor { shape: CROSSHAIR },
            },
        );
    }
    Scene::new(SceneParts {
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: SCREEN_W as f64,
            height: SCREEN_H as f64,
        },
        duration_ms: 6_000.0,
        events: &events,
        settings,
        cursors,
        background: None,
    })
}

fn goldens_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

fn rgba(format_bgra: Vec<u8>) -> Vec<u8> {
    let mut pixels = format_bgra;
    for px in pixels.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    pixels
}

struct Diff {
    mean: f64,
    off: usize,
}

fn compare(actual: &[u8], expected: &[u8]) -> Diff {
    let mut total = 0u64;
    let mut off = 0;
    for (a, e) in actual.chunks_exact(4).zip(expected.chunks_exact(4)) {
        let worst = (0..3).map(|c| a[c].abs_diff(e[c])).max().unwrap_or(0);
        total += (0..3).map(|c| a[c].abs_diff(e[c]) as u64).sum::<u64>();
        if worst > 24 {
            off += 1;
        }
    }
    Diff {
        mean: total as f64 / (actual.len() / 4 * 3) as f64,
        off,
    }
}

fn check(name: &str, pixels: &[u8]) {
    let path = goldens_dir().join(format!("{name}.png"));
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(goldens_dir()).unwrap();
        bitmap::save_png(&path, OUT_W, OUT_H, pixels).unwrap();
        return;
    }
    let (w, h, expected) = bitmap::load_png(&path)
        .unwrap_or_else(|e| panic!("{e}; run `make update-goldens` to create it"));
    assert_eq!((w, h), (OUT_W, OUT_H), "{name}: size");
    let diff = compare(pixels, &expected);
    let allowed = (OUT_W * OUT_H) as usize / 500;
    if diff.mean > 1.0 || diff.off > allowed {
        let failures = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("golden-failures");
        std::fs::create_dir_all(&failures).unwrap();
        let actual = failures.join(format!("{name}.png"));
        bitmap::save_png(&actual, OUT_W, OUT_H, pixels).unwrap();
        panic!(
            "{name}: mean difference {:.3}, {} pixels off (allowed {allowed}); actual frame at {}",
            diff.mean,
            diff.off,
            actual.display()
        );
    }
}

fn render(compositor: &mut Compositor, scene: &Scene, t_ms: f64) -> Vec<u8> {
    rgba(compositor.frame(scene, &mut Pattern::new(), t_ms).unwrap())
}

#[test]
fn golden_frames() {
    let mut compositor = Compositor::new(OUT_W, OUT_H, PixelFormat::Bgra8).unwrap();
    let scene = fixture(EditSettings::default());
    let zoom = |t: f64| scene.timeline.camera(t).scale;
    assert_eq!(zoom(600.0), 1.0);
    assert!(zoom(1_250.0) > 1.2 && zoom(1_250.0) < 1.8);
    assert!(zoom(2_700.0) > 1.99);

    for (name, t) in [
        ("no-zoom", 600.0),
        ("zoom-in", 1_250.0),
        ("click-ripple", 1_650.0),
        ("zoomed-following", 2_700.0),
        ("pressed", 2_840.0),
    ] {
        check(name, &render(&mut compositor, &scene, t));
    }

    check(
        "recorded-cursor-zoomed",
        &render(
            &mut compositor,
            &fixture_with_cursor(EditSettings::default(), Some(crosshair())),
            2_700.0,
        ),
    );

    let mut plain = EditSettings::default();
    plain.background.fill = BackgroundFill::Solid {
        color: Color::rgb(0x20, 0x24, 0x2c),
    };
    plain.background.corner_radius = 0.05;
    plain.background.shadow.opacity = 0.0;
    plain.cursor.size = 3.0;
    plain.zoom.auto = false;
    check(
        "solid-background-large-cursor",
        &render(&mut compositor, &fixture(plain), 1_000.0),
    );
}

#[test]
fn frame_lands_where_the_layout_says() {
    let mut settings = EditSettings::default();
    settings.background.fill = BackgroundFill::Solid {
        color: Color::rgb(10, 200, 30),
    };
    settings.background.shadow.opacity = 0.0;
    settings.background.corner_radius = 0.0;
    settings.zoom.auto = false;
    let scene = fixture(settings);
    let mut compositor = Compositor::new(OUT_W, OUT_H, PixelFormat::Rgba8).unwrap();
    let pixels = compositor.frame(&scene, &mut Pattern::new(), 0.0).unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * OUT_W + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let near = |a: [u8; 3], b: [u8; 3]| (0..3).all(|c| a[c].abs_diff(b[c]) <= 2);
    assert!(near(at(2, 2), [10, 200, 30]), "{:?}", at(2, 2));

    let content = compositor.layout(&scene).content;
    let sx = 425.0;
    let sy = 260.0;
    let x = (content.x + (sx + 0.5) / SCREEN_W as f64 * content.width) as u32;
    let y = (content.y + (sy + 0.5) / SCREEN_H as f64 * content.height) as u32;
    assert!(
        near(at(x, y), Pattern::color(sx as u32, sy as u32)),
        "{:?} vs {:?}",
        at(x, y),
        Pattern::color(sx as u32, sy as u32)
    );
}

#[test]
fn recorded_cursor_is_placed_by_its_hotspot_and_scaled_in_points() {
    let mut settings = EditSettings::default();
    settings.zoom.auto = false;
    settings.cursor.smoothing = 0.0;
    settings.cursor.size = 3.0;
    settings.clicks.squish = false;
    let scene = fixture_with_cursor(settings, Some(crosshair()));
    let mut compositor = Compositor::new(OUT_W, OUT_H, PixelFormat::Rgba8).unwrap();
    // The cursor rests at (300, 200) between 1 s and the click at 1.5 s.
    let pixels = compositor
        .frame(&scene, &mut Pattern::new(), 1_200.0)
        .unwrap();
    let at = |x: f64, y: f64| {
        let i = ((y.round() as u32 * OUT_W + x.round() as u32) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let near = |a: [u8; 3], b: [u8; 3]| (0..3).all(|c| a[c].abs_diff(b[c]) <= 12);

    let content = compositor.layout(&scene).content;
    let hx = content.x + 300.0 / SCREEN_W as f64 * content.width;
    let hy = content.y + 200.0 / SCREEN_H as f64 * content.height;
    let px_per_pt = content.width / SCREEN_W as f64 * 3.0;
    assert!(near(at(hx, hy), [230, 20, 20]), "hotspot: {:?}", at(hx, hy));
    for (dx, dy) in [(7.5, 0.0), (-7.5, 0.0), (0.0, 7.5), (0.0, -7.5)] {
        let p = at(hx + dx * px_per_pt, hy + dy * px_per_pt);
        assert!(near(p, [20, 40, 230]), "ring at ({dx}, {dy}) pt: {p:?}");
    }
    let outside = at(hx + 11.0 * px_per_pt, hy);
    assert!(!near(outside, [20, 40, 230]) && !near(outside, [230, 20, 20]));
}
