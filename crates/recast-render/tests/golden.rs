//! Renders a small fixture at fixed times and compares the frames with the PNGs in
//! `tests/goldens`. Run with `UPDATE_GOLDENS=1` (`make update-goldens`) to rewrite them.

use std::{collections::HashMap, path::PathBuf};

use recast_project::{
    Annotation, AnnotationShape, Annotations, BackgroundFill, BackgroundSettings, Color,
    EditSettings, EventKind, EventLog, InputEvent, MouseButton, Rect, Shadow, ZoomFocus,
    ZoomSegment,
};
use recast_render::{
    Compositor, CpuFrame, FrameSource, PixelFormat, Result, Scene, SceneParts,
    annotations::Raster,
    bitmap::{self, Rgba},
    cursor::CursorImage,
    still::StillRenderer,
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
            has_alpha: false,
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
        video_size: (SCREEN_W, SCREEN_H),
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
    check_sized(name, OUT_W, OUT_H, pixels);
}

fn check_sized(name: &str, width: u32, height: u32, pixels: &[u8]) {
    let path = goldens_dir().join(format!("{name}.png"));
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(goldens_dir()).unwrap();
        bitmap::save_png(&path, width, height, pixels).unwrap();
        return;
    }
    let (w, h, expected) = bitmap::load_png(&path)
        .unwrap_or_else(|e| panic!("{e}; run `make update-goldens` to create it"));
    assert_eq!((w, h), (width, height), "{name}: size");
    let diff = compare(pixels, &expected);
    let allowed = (width * height) as usize / 500;
    if diff.mean > 1.0 || diff.off > allowed {
        let failures = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("golden-failures");
        std::fs::create_dir_all(&failures).unwrap();
        let actual = failures.join(format!("{name}.png"));
        bitmap::save_png(&actual, width, height, pixels).unwrap();
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

const SHOT_W: u32 = 240;
const SHOT_H: u32 = 160;
const SHOT_CORNER: f64 = 14.0;

/// A window screenshot as ScreenCaptureKit returns it: straight RGBA with transparent
/// rounded corners and no shadow.
fn window_shot() -> Vec<u8> {
    let mut pixels = Vec::with_capacity((SHOT_W * SHOT_H * 4) as usize);
    for y in 0..SHOT_H {
        for x in 0..SHOT_W {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let cx = px.clamp(SHOT_CORNER, SHOT_W as f64 - SHOT_CORNER);
            let cy = py.clamp(SHOT_CORNER, SHOT_H as f64 - SHOT_CORNER);
            let outside = (px - cx).hypot(py - cy) - SHOT_CORNER;
            let alpha = (0.5 - outside).clamp(0.0, 1.0);
            let [r, g, b] = Pattern::color(x * 2 + 60, y * 2 + 40);
            pixels.extend_from_slice(&[r, g, b, (alpha * 255.0).round() as u8]);
        }
    }
    pixels
}

fn beautify_background() -> BackgroundSettings {
    BackgroundSettings {
        padding: 0.12,
        corner_radius: 0.06,
        ..Default::default()
    }
}

#[test]
fn beautified_screenshot_golden() {
    let mut renderer = StillRenderer::new(SHOT_W, SHOT_H, window_shot());
    let background = beautify_background();
    let (w, h) = renderer.native_size(&background);
    assert_eq!((w, h), (278, 198));
    let pixels = renderer.render(&background, w, h).unwrap();
    check_sized("beautify-window", w, h, &pixels);

    let mut solid = background.clone();
    solid.fill = BackgroundFill::Solid {
        color: Color::rgb(0xf4, 0xf4, 0xf5),
    };
    solid.shadow.opacity = 0.8;
    solid.shadow.blur = 0.08;
    let pixels = renderer.render(&solid, w, h).unwrap();
    check_sized("beautify-solid-shadow", w, h, &pixels);
}

#[test]
fn beautify_at_native_size_keeps_every_pixel() {
    let shot = window_shot();
    let mut renderer = StillRenderer::new(SHOT_W, SHOT_H, shot.clone());
    let background = BackgroundSettings {
        fill: BackgroundFill::Solid {
            color: Color::rgb(0, 0, 0),
        },
        padding: 0.0,
        corner_radius: 0.0,
        shadow: Shadow {
            opacity: 0.0,
            ..Default::default()
        },
    };
    let (w, h) = renderer.native_size(&background);
    assert_eq!((w, h), (SHOT_W, SHOT_H));
    let pixels = renderer.render(&background, w, h).unwrap();
    for (i, (out, src)) in pixels.chunks_exact(4).zip(shot.chunks_exact(4)).enumerate() {
        let a = src[3] as f64 / 255.0;
        for c in 0..3 {
            let expected = src[c] as f64 * a;
            assert!(
                (out[c] as f64 - expected).abs() <= 2.0,
                "pixel {i} channel {c}: {} vs {expected}",
                out[c]
            );
        }
        assert_eq!(out[3], 255);
    }
}

#[test]
fn beautify_padding_puts_the_corners_on_the_background() {
    let mut renderer = StillRenderer::new(SHOT_W, SHOT_H, window_shot());
    let mut background = beautify_background();
    background.fill = BackgroundFill::Solid {
        color: Color::rgb(10, 200, 30),
    };
    background.shadow.opacity = 0.0;
    background.corner_radius = 0.0;
    let (w, h) = renderer.native_size(&background);
    let pixels = renderer.render(&background, w, h).unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let near = |a: [u8; 3], b: [u8; 3]| (0..3).all(|c| a[c].abs_diff(b[c]) <= 2);
    let pad = (0.12 * SHOT_H as f64).round() as u32;
    assert!(near(at(2, 2), [10, 200, 30]));
    assert!(
        near(at(pad + 1, pad + 1), [10, 200, 30]),
        "transparent corner shows the background: {:?}",
        at(pad + 1, pad + 1)
    );
    let [r, g, b] = Pattern::color(100 * 2 + 60, 80 * 2 + 40);
    assert!(near(at(pad + 100, pad + 80), [r, g, b]));
}

/// One-pixel black and white checks, the worst case for any resampling.
fn checkerboard(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let v = if (x + y) % 2 == 0 { 0 } else { 255 };
            pixels.extend_from_slice(&[v, v, v, 255]);
        }
    }
    pixels
}

#[test]
fn beautify_at_native_size_is_pixel_exact_with_padding() {
    let sizes = [
        (240, 160, 0.12),
        (300, 200, 0.1),
        (2431, 1517, 0.08),
        (3024, 1964, 0.08),
        (157, 311, 0.05),
        (101, 57, 0.3),
    ];
    for (width, height, padding) in sizes {
        let source = checkerboard(width, height);
        let mut renderer = StillRenderer::new(width, height, source.clone());
        let background = BackgroundSettings {
            fill: BackgroundFill::Solid {
                color: Color::rgb(200, 30, 30),
            },
            padding,
            corner_radius: 0.0,
            shadow: Shadow {
                opacity: 0.0,
                ..Default::default()
            },
        };
        let (out_w, out_h) = renderer.native_size(&background);
        let pixels = renderer.render(&background, out_w, out_h).unwrap();
        let pad = (out_w - width) / 2;
        assert_eq!(out_h - height, 2 * pad, "{width}×{height}: even padding");
        let mut blurred = 0;
        for y in 0..height {
            for x in 0..width {
                let out = (((y + pad) * out_w + x + pad) * 4) as usize;
                let src = ((y * width + x) * 4) as usize;
                if pixels[out].abs_diff(source[src]) > 1 {
                    blurred += 1;
                }
            }
        }
        assert_eq!(
            blurred, 0,
            "{width}×{height} with padding {padding}: {blurred} pixels resampled"
        );
    }
}

#[test]
fn a_replaced_image_of_the_same_size_is_rendered() {
    let (width, height) = (64, 48);
    let background = BackgroundSettings {
        padding: 0.0,
        corner_radius: 0.0,
        ..Default::default()
    };
    let mut renderer = StillRenderer::new(width, height, vec![0; (width * height * 4) as usize]);
    let transparent = renderer.render(&background, width, height).unwrap();

    let source = checkerboard(width, height);
    renderer.set_image(width, height, source.clone());
    let pixels = renderer.render(&background, width, height).unwrap();
    assert_ne!(pixels, transparent);
    let off = pixels
        .chunks_exact(4)
        .zip(source.chunks_exact(4))
        .filter(|(out, src)| out[0].abs_diff(src[0]) > 1)
        .count();
    assert_eq!(off, 0);

    renderer.set_image(32, 16, checkerboard(32, 16));
    assert_eq!(renderer.image_size(), (32, 16));
    assert_eq!(
        renderer.render(&background, 32, 16).unwrap().len(),
        32 * 16 * 4
    );
}

/// Black-on-white strokes one to three pixels wide, like small text.
fn strokes(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let ink = (x % 7 < 1 + y / 20 % 3) || (y % 9 == 0 && x % 23 < 15);
            let v = if ink { 20 } else { 240 };
            pixels.extend_from_slice(&[v, v, v, 255]);
        }
    }
    pixels
}

/// Bilinear enlargement by 2, sampling at output pixel centers like a GPU sampler.
fn bilinear_2x(pixels: &[u8], width: u32, height: u32) -> Vec<u8> {
    let at = |x: i64, y: i64| {
        let x = x.clamp(0, width as i64 - 1) as u32;
        let y = y.clamp(0, height as i64 - 1) as u32;
        pixels[((y * width + x) * 4) as usize] as f64
    };
    let mut out = Vec::with_capacity((width * height * 16) as usize);
    for oy in 0..height * 2 {
        for ox in 0..width * 2 {
            let (sx, sy) = ((ox as f64 + 0.5) / 2.0 - 0.5, (oy as f64 + 0.5) / 2.0 - 0.5);
            let (x0, y0) = (sx.floor(), sy.floor());
            let (fx, fy) = (sx - x0, sy - y0);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let top = at(x0, y0) * (1.0 - fx) + at(x0 + 1, y0) * fx;
            let bottom = at(x0, y0 + 1) * (1.0 - fx) + at(x0 + 1, y0 + 1) * fx;
            let v = (top * (1.0 - fy) + bottom * fy).round() as u8;
            out.extend_from_slice(&[v, v, v, 255]);
        }
    }
    out
}

/// Mean squared difference between horizontal and vertical neighbors of the first channel.
fn sharpness(pixels: &[u8], width: u32, height: u32) -> f64 {
    let at = |x: u32, y: u32| pixels[((y * width + x) * 4) as usize] as f64;
    let mut sum = 0.0;
    for y in 0..height - 1 {
        for x in 0..width - 1 {
            sum += (at(x + 1, y) - at(x, y)).powi(2) + (at(x, y + 1) - at(x, y)).powi(2);
        }
    }
    sum / ((width - 1) * (height - 1)) as f64
}

/// Mean absolute difference from the source enlarged with nearest neighbor.
fn error_from_nearest(pixels: &[u8], source: &[u8], width: u32, height: u32) -> f64 {
    let mut sum = 0.0;
    for y in 0..height * 2 {
        for x in 0..width * 2 {
            let out = pixels[((y * width * 2 + x) * 4) as usize] as f64;
            let src = source[(((y / 2) * width + x / 2) * 4) as usize] as f64;
            sum += (out - src).abs();
        }
    }
    sum / (width * height * 4) as f64
}

#[test]
fn enlarging_is_sharper_than_bilinear() {
    let (width, height) = (240, 160);
    let source = strokes(width, height);
    let mut renderer = StillRenderer::new(width, height, source.clone());
    let background = BackgroundSettings {
        padding: 0.0,
        corner_radius: 0.0,
        shadow: Shadow {
            opacity: 0.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let pixels = renderer.render(&background, width * 2, height * 2).unwrap();
    let bilinear = bilinear_2x(&source, width, height);
    let (ours, theirs) = (
        sharpness(&pixels, width * 2, height * 2),
        sharpness(&bilinear, width * 2, height * 2),
    );
    let (our_error, their_error) = (
        error_from_nearest(&pixels, &source, width, height),
        error_from_nearest(&bilinear, &source, width, height),
    );
    assert!(
        ours > theirs * 1.2,
        "sharpness {ours:.0} vs bilinear {theirs:.0}"
    );
    assert!(
        our_error < their_error * 0.85,
        "error {our_error:.1} vs bilinear {their_error:.1}"
    );
}

#[test]
fn auto_resolution_shows_every_video_pixel_once() {
    let (width, height) = (640, 400);
    let mut settings = EditSettings::default();
    settings.zoom.auto = false;
    settings.background.shadow.opacity = 0.0;
    settings.background.corner_radius = 0.0;
    let scene = Scene::new(SceneParts {
        bounds: Rect {
            x: 100.0,
            y: 50.0,
            width: width as f64 / 2.0,
            height: height as f64 / 2.0,
        },
        video_size: (width, height),
        duration_ms: 1_000.0,
        events: &EventLog::default(),
        settings,
        cursors: HashMap::new(),
        background: None,
    });
    let (out_w, out_h) = scene.output_size();
    let pad = (0.08 * height as f64).round() as u32;
    assert_eq!((out_w, out_h), (width + 2 * pad, height + 2 * pad));

    struct Checks(Vec<u8>);
    impl FrameSource for Checks {
        fn frame_at(&mut self, _t_ms: f64) -> Result<CpuFrame<'_>> {
            Ok(CpuFrame {
                width: 640,
                height: 400,
                bytes_per_row: 640 * 4,
                format: PixelFormat::Rgba8,
                data: &self.0,
                id: Some(1),
                has_alpha: false,
            })
        }
    }
    let source = checkerboard(width, height);
    let mut compositor = Compositor::new(out_w, out_h, PixelFormat::Rgba8).unwrap();
    let pixels = compositor
        .frame(&scene, &mut Checks(source.clone()), 0.0)
        .unwrap();
    let mut resampled = 0;
    for y in 0..height {
        for x in 0..width {
            let out = (((y + pad) * out_w + x + pad) * 4) as usize;
            if pixels[out].abs_diff(source[((y * width + x) * 4) as usize]) > 1 {
                resampled += 1;
            }
        }
    }
    assert_eq!(resampled, 0);
}

/// Settings that show the recording 1:1 in a frame of its size, without effects.
fn plain_settings() -> EditSettings {
    let mut settings = EditSettings::default();
    settings.zoom.auto = false;
    settings.background.padding = 0.0;
    settings.background.corner_radius = 0.0;
    settings.background.shadow.opacity = 0.0;
    settings
}

fn annotated(mut settings: EditSettings, shapes: Vec<(AnnotationShape, f64, f64)>) -> Scene {
    settings.annotations = Annotations {
        tracks: 1,
        items: shapes
            .into_iter()
            .enumerate()
            .map(|(i, (shape, start_ms, end_ms))| Annotation {
                id: format!("a{i}"),
                start_ms,
                end_ms,
                track: 0,
                shape,
            })
            .collect(),
    };
    Scene::new(SceneParts {
        bounds: rect(0.0, 0.0, SCREEN_W as f64, SCREEN_H as f64),
        video_size: (SCREEN_W, SCREEN_H),
        duration_ms: 6_000.0,
        events: &EventLog::default(),
        settings,
        cursors: HashMap::new(),
        background: None,
    })
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// Renders at the recording's own size, so each output pixel is one screen texel.
fn render_native(scene: &Scene, t_ms: f64) -> Vec<u8> {
    let mut compositor = Compositor::new(SCREEN_W, SCREEN_H, PixelFormat::Rgba8).unwrap();
    compositor.frame(scene, &mut Pattern::new(), t_ms).unwrap()
}

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [f64; 3] {
    let i = ((y * width + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2]].map(|v| v as f64)
}

/// Averages of `block × block` cells of the pattern in `area` (texels), as the screenshot
/// editor computes them.
fn cell_averages(area: (u32, u32, u32, u32), block: u32) -> Vec<Vec<[f64; 3]>> {
    let (x0, y0, w, h) = area;
    (0..h.div_ceil(block))
        .map(|row| {
            (0..w.div_ceil(block))
                .map(|column| {
                    let mut sum = [0.0; 3];
                    let mut n = 0.0;
                    for y in y0 + row * block..(y0 + (row + 1) * block).min(y0 + h) {
                        for x in x0 + column * block..(x0 + (column + 1) * block).min(x0 + w) {
                            let c = Pattern::color(x, y);
                            for i in 0..3 {
                                sum[i] += c[i] as f64;
                            }
                            n += 1.0;
                        }
                    }
                    sum.map(|v| v / n)
                })
                .collect()
        })
        .collect()
}

/// The screenshot editor's smoothing: neighbours weighted 1-2-1 on both axes, twice.
fn smooth(cells: &[Vec<[f64; 3]>]) -> Vec<Vec<[f64; 3]>> {
    let (rows, columns) = (cells.len() as i64, cells[0].len() as i64);
    let pass = |input: &Vec<Vec<[f64; 3]>>, dc: i64, dr: i64| {
        let at =
            |r: i64, c: i64| input[r.clamp(0, rows - 1) as usize][c.clamp(0, columns - 1) as usize];
        (0..rows)
            .map(|r| {
                (0..columns)
                    .map(|c| {
                        let (a, b, d) = (at(r - dr, c - dc), at(r, c), at(r + dr, c + dc));
                        [0, 1, 2].map(|i| (a[i] + 2.0 * b[i] + d[i]) / 4.0)
                    })
                    .collect()
            })
            .collect::<Vec<Vec<[f64; 3]>>>()
    };
    let once = |v: &Vec<Vec<[f64; 3]>>| pass(&pass(v, 1, 0), 0, 1);
    once(&once(&cells.to_vec()))
}

fn close(a: [f64; 3], b: [f64; 3], tolerance: f64) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() <= tolerance)
}

#[test]
fn pixelate_shows_the_cell_averages_of_the_recording() {
    let shape = AnnotationShape::Pixelate {
        rect: rect(100.0, 60.0, 100.0, 50.0),
        block: 12.0,
    };
    let scene = annotated(plain_settings(), vec![(shape, 0.0, 6_000.0)]);
    let pixels = render_native(&scene, 500.0);
    let cells = cell_averages((100, 60, 100, 50), 12);
    for y in 60..110 {
        for x in 100..200 {
            let expected = cells[((y - 60) / 12) as usize][((x - 100) / 12) as usize];
            let actual = pixel(&pixels, SCREEN_W, x, y);
            assert!(
                close(actual, expected, 1.5),
                "({x}, {y}): {actual:?} vs {expected:?}"
            );
        }
    }
    let outside = pixel(&pixels, SCREEN_W, 99, 59);
    assert!(close(
        outside,
        Pattern::color(99, 59).map(|v| v as f64),
        1.5
    ));
}

#[test]
fn blur_matches_the_screenshot_editor_and_removes_detail() {
    let area = (80, 70, 150, 90);
    let shape = AnnotationShape::Blur {
        rect: rect(80.0, 70.0, 150.0, 90.0),
        block: 12.0,
    };
    let scene = annotated(plain_settings(), vec![(shape, 0.0, 6_000.0)]);
    let pixels = render_native(&scene, 500.0);
    let smoothed = smooth(&cell_averages(area, 12));
    let (rows, columns) = (smoothed.len(), smoothed[0].len());
    let along = |position: u32, count: usize| {
        let g = ((position as f64 + 0.5) / 12.0 - 0.5).clamp(0.0, (count - 1) as f64);
        let i0 = g.floor() as usize;
        (i0, (i0 + 1).min(count - 1), g - g.floor())
    };
    let mut values = Vec::new();
    for y in 0..area.3 {
        for x in 0..area.2 {
            let (c0, c1, tx) = along(x, columns);
            let (r0, r1, ty) = along(y, rows);
            let lerp =
                |a: [f64; 3], b: [f64; 3], t: f64| [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t);
            let expected = lerp(
                lerp(smoothed[r0][c0], smoothed[r0][c1], tx),
                lerp(smoothed[r1][c0], smoothed[r1][c1], tx),
                ty,
            );
            let actual = pixel(&pixels, SCREEN_W, area.0 + x, area.1 + y);
            assert!(
                close(actual, expected, 2.0),
                "({x}, {y}): {actual:?} vs {expected:?}"
            );
            values.push(actual[0]);
        }
    }
    let variance = |v: &[f64]| {
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64
    };
    let original: Vec<f64> = (0..area.3)
        .flat_map(|y| (0..area.2).map(move |x| Pattern::color(area.0 + x, area.1 + y)[0] as f64))
        .collect();
    assert!(variance(&values) < variance(&original) / 10.0);
}

#[test]
fn annotations_follow_the_recording_when_zoomed() {
    let mut settings = plain_settings();
    settings.zoom.segments = vec![ZoomSegment {
        start_ms: 0.0,
        end_ms: 6_000.0,
        level: 2.0,
        focus: ZoomFocus::Point { x: 0.5, y: 0.5 },
    }];
    let marker = AnnotationShape::Rect {
        rect: rect(300.0, 200.0, 10.0, 5.0),
        color: Color::rgb(255, 0, 0),
        width: 1.0,
        fill: true,
    };
    let pixelate = AnnotationShape::Pixelate {
        rect: rect(200.0, 120.0, 48.0, 24.0),
        block: 24.0,
    };
    let scene = annotated(
        settings,
        vec![(marker.clone(), 0.0, 6_000.0), (pixelate, 0.0, 6_000.0)],
    );
    // A red image two pixels per point, a point larger than the marker on each side.
    let red = [230u8, 20, 30, 255].repeat(24 * 14);
    scene.rasters.insert(
        marker.look_key().unwrap(),
        Raster::new(24, 14, red, (-1.0, -1.0), (12.0, 7.0)).unwrap(),
    );
    let mut compositor = Compositor::new(SCREEN_W, SCREEN_H, PixelFormat::Rgba8).unwrap();
    let pixels = compositor
        .frame(&scene, &mut Pattern::new(), 3_000.0)
        .unwrap();
    assert_eq!(compositor.missing_rasters(), 0);
    let camera = scene.timeline.camera(3_000.0);
    assert!((camera.scale - 2.0).abs() < 1e-6);
    // The view shows screen points 160..480 × 100..300 at 2 output pixels per point.
    let out = |x: f64, y: f64| (((x - 160.0) * 2.0) as u32, ((y - 100.0) * 2.0) as u32);
    let (x, y) = out(305.0, 202.5);
    assert!(close(
        pixel(&pixels, SCREEN_W, x, y),
        [230.0, 20.0, 30.0],
        3.0
    ));
    let (x, y) = out(299.5, 199.5);
    assert!(close(
        pixel(&pixels, SCREEN_W, x, y),
        [230.0, 20.0, 30.0],
        3.0
    ));
    let (x, y) = out(311.5, 206.5);
    assert!(!close(
        pixel(&pixels, SCREEN_W, x, y),
        [230.0, 20.0, 30.0],
        30.0
    ));

    // Cells stay 24 recorded pixels wide: 48 output pixels when zoomed in twice.
    let cells = cell_averages((200, 120, 48, 24), 24);
    for (column, sx) in [(0, 205.0), (1, 240.0)] {
        let (x, y) = out(sx, 130.0);
        assert!(
            close(pixel(&pixels, SCREEN_W, x, y), cells[0][column], 1.5),
            "cell {column}"
        );
    }
}

#[test]
fn annotations_show_only_in_their_time_range() {
    let marker = AnnotationShape::Highlight {
        rect: rect(10.0, 10.0, 50.0, 30.0),
        color: Color::rgb(255, 214, 10),
    };
    let blur = AnnotationShape::Blur {
        rect: rect(300.0, 200.0, 100.0, 60.0),
        block: 12.0,
    };
    let mut scene = annotated(
        plain_settings(),
        vec![(marker.clone(), 1_000.0, 2_000.0), (blur, 1_000.0, 2_000.0)],
    );
    let plain = render_native(&scene, 500.0);
    let mut compositor = Compositor::new(SCREEN_W, SCREEN_H, PixelFormat::Rgba8).unwrap();
    compositor
        .frame(&scene, &mut Pattern::new(), 1_500.0)
        .unwrap();
    assert_eq!(compositor.missing_rasters(), 1);
    assert_eq!(scene.missing_rasters(), ["a0"]);

    scene.rasters.insert(
        marker.look_key().unwrap(),
        Raster::new(1, 1, vec![255, 214, 10, 102], (0.0, 0.0), (50.0, 30.0)).unwrap(),
    );
    let shown = compositor
        .frame(&scene, &mut Pattern::new(), 1_500.0)
        .unwrap();
    assert_eq!(compositor.missing_rasters(), 0);
    assert!(scene.missing_rasters().is_empty());
    let differs = |a: &[u8], x: u32, y: u32| {
        !close(pixel(a, SCREEN_W, x, y), pixel(&plain, SCREEN_W, x, y), 3.0)
    };
    assert!(differs(&shown, 30, 20) && differs(&shown, 350, 220));
    for t in [999.0, 2_001.0] {
        let hidden = compositor.frame(&scene, &mut Pattern::new(), t).unwrap();
        assert!(
            !differs(&hidden, 30, 20) && !differs(&hidden, 350, 220),
            "{t}"
        );
    }

    scene.hidden = Some("a0".into());
    let without = compositor
        .frame(&scene, &mut Pattern::new(), 1_500.0)
        .unwrap();
    assert!(!differs(&without, 30, 20) && differs(&without, 350, 220));
    assert!(scene.missing_rasters().is_empty());
}

fn red_marker() -> (AnnotationShape, Raster) {
    let shape = AnnotationShape::Rect {
        rect: rect(100.0, 100.0, 80.0, 40.0),
        color: Color::rgb(230, 20, 30),
        width: 1.0,
        fill: true,
    };
    let raster = Raster::new(1, 1, vec![230, 20, 30, 255], (0.0, 0.0), (80.0, 40.0)).unwrap();
    (shape, raster)
}

#[test]
fn blur_on_a_later_track_covers_the_marks_under_it() {
    let (marker, raster) = red_marker();
    let blur = AnnotationShape::Blur {
        rect: rect(140.0, 90.0, 100.0, 60.0),
        block: 12.0,
    };
    let red = [230.0, 20.0, 30.0];
    for (mark_track, blur_track, covered) in [(0, 1, true), (1, 0, false)] {
        let mut scene = annotated(plain_settings(), vec![]);
        let mut settings = scene.settings.clone();
        settings.annotations = Annotations {
            tracks: 2,
            items: vec![
                Annotation {
                    id: "mark".into(),
                    start_ms: 0.0,
                    end_ms: 6_000.0,
                    track: mark_track,
                    shape: marker.clone(),
                },
                Annotation {
                    id: "blur".into(),
                    start_ms: 0.0,
                    end_ms: 6_000.0,
                    track: blur_track,
                    shape: blur.clone(),
                },
            ],
        };
        scene.set_settings(&EventLog::default(), settings);
        scene
            .rasters
            .insert(marker.look_key().unwrap(), raster.clone());
        let pixels = render_native(&scene, 500.0);
        assert!(close(pixel(&pixels, SCREEN_W, 120, 120), red, 3.0));
        let inside = pixel(&pixels, SCREEN_W, 160, 120);
        assert_eq!(
            !close(inside, red, 30.0),
            covered,
            "blur on track {blur_track}: {inside:?}"
        );
    }
}

#[test]
fn a_changed_annotation_keeps_its_image_until_the_new_one_arrives() {
    let (marker, raster) = red_marker();
    let mut scene = annotated(plain_settings(), vec![(marker.clone(), 0.0, 6_000.0)]);
    scene.rasters.insert(marker.look_key().unwrap(), raster);
    let mut compositor = Compositor::new(SCREEN_W, SCREEN_H, PixelFormat::Rgba8).unwrap();
    compositor.frame(&scene, &mut Pattern::new(), 0.0).unwrap();

    let mut settings = scene.settings.clone();
    let AnnotationShape::Rect { color, .. } = &mut settings.annotations.items[0].shape else {
        unreachable!()
    };
    *color = Color::rgb(0, 0, 255);
    scene.set_settings(&EventLog::default(), settings);
    let pixels = compositor.frame(&scene, &mut Pattern::new(), 0.0).unwrap();
    assert_eq!(compositor.missing_rasters(), 1);
    assert!(close(
        pixel(&pixels, SCREEN_W, 120, 120),
        [230.0, 20.0, 30.0],
        3.0
    ));

    // Edited in the editor while hidden, it doesn't come back with the old image.
    scene.hidden = Some("a0".into());
    compositor.frame(&scene, &mut Pattern::new(), 0.0).unwrap();
    scene.hidden = None;
    let pixels = compositor.frame(&scene, &mut Pattern::new(), 0.0).unwrap();
    assert_eq!(compositor.missing_rasters(), 1);
    assert!(!close(
        pixel(&pixels, SCREEN_W, 120, 120),
        [230.0, 20.0, 30.0],
        30.0
    ));
}
