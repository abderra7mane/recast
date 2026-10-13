//! Measures what annotations cost the compositor: frames of a synthetic Retina recording
//! rendered and read back, as an export does, with and without annotations.
//!
//! usage: annotation-bench [FRAMES]

use std::{collections::HashMap, time::Instant};

use recast_project::{
    Annotation, AnnotationShape, Annotations, Color, EditSettings, EventLog, Point, Rect,
    ZoomFocus, ZoomSegment,
};
use recast_render::{
    Compositor, CpuFrame, FrameSource, PixelFormat, Result, Scene, SceneParts, annotations::Raster,
};

const SCREEN: (f64, f64) = (1512.0, 982.0);
const VIDEO: (u32, u32) = (3024, 1964);

struct Desktop {
    frames: Vec<Vec<u8>>,
    next: u64,
}

impl FrameSource for Desktop {
    fn frame_at(&mut self, _t_ms: f64) -> Result<CpuFrame<'_>> {
        // A new frame every time, as during playback, so nothing is skipped as unchanged.
        self.next += 1;
        let data = &self.frames[(self.next % 2) as usize];
        Ok(CpuFrame {
            width: VIDEO.0,
            height: VIDEO.1,
            bytes_per_row: VIDEO.0 as usize * 4,
            format: PixelFormat::Bgra8,
            data,
            id: Some(self.next),
            has_alpha: false,
        })
    }
}

fn desktop() -> Desktop {
    let frame = |shift: u32| {
        let mut pixels = Vec::with_capacity((VIDEO.0 * VIDEO.1 * 4) as usize);
        for y in 0..VIDEO.1 {
            for x in 0..VIDEO.0 {
                let ink = (y + shift) % 36 < 12 && (x / 14) % 9 != 0;
                let v = if ink { 40 } else { 236 };
                pixels.extend_from_slice(&[v, v, (v as u32 + x / 20 % 20) as u8, 255]);
            }
        }
        pixels
    };
    Desktop {
        frames: vec![frame(0), frame(6)],
        next: 0,
    }
}

fn rect(i: usize, width: f64, height: f64) -> Rect {
    Rect {
        x: 40.0 + (i % 10) as f64 * 140.0,
        y: 40.0 + (i / 10 % 6) as f64 * 150.0,
        width,
        height,
    }
}

/// `marks` drawn annotations of 110 × 70 points and `hidden` blurred and pixelated areas
/// of `area` points, all showing the whole time.
fn scene(marks: usize, hidden: usize, area: (f64, f64), zoom: bool) -> Scene {
    let mut items = Vec::new();
    for i in 0..marks {
        let shape = if i % 2 == 0 {
            AnnotationShape::Rect {
                rect: rect(i, 110.0, 70.0),
                color: Color::rgb(255, 59, 48),
                width: 3.0,
                fill: false,
            }
        } else {
            AnnotationShape::Arrow {
                from: Point {
                    x: rect(i, 0.0, 0.0).x,
                    y: rect(i, 0.0, 0.0).y,
                },
                to: Point {
                    x: rect(i, 0.0, 0.0).x + 110.0,
                    y: rect(i, 0.0, 0.0).y + 70.0,
                },
                color: Color::rgb(10, 132, 255),
                width: 4.0,
            }
        };
        items.push(shape);
    }
    for i in 0..hidden {
        let rect = rect(i * 3 + 1, area.0, area.1);
        items.push(if i % 2 == 0 {
            AnnotationShape::Blur { rect, block: 12.0 }
        } else {
            AnnotationShape::Pixelate { rect, block: 12.0 }
        });
    }
    let mut settings = EditSettings::default();
    settings.zoom.auto = false;
    if zoom {
        settings.zoom.segments = vec![ZoomSegment {
            start_ms: 0.0,
            end_ms: 60_000.0,
            level: 2.0,
            focus: ZoomFocus::Point { x: 0.4, y: 0.4 },
        }];
    }
    settings.annotations = Annotations {
        tracks: 3,
        items: items
            .into_iter()
            .enumerate()
            .map(|(i, shape)| Annotation {
                id: format!("a{i}"),
                start_ms: 0.0,
                end_ms: 60_000.0,
                track: (i % 3) as u32,
                shape,
            })
            .collect(),
    };
    let scene = Scene::new(SceneParts {
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: SCREEN.0,
            height: SCREEN.1,
        },
        video_size: VIDEO,
        duration_ms: 60_000.0,
        events: &EventLog::default(),
        settings,
        cursors: HashMap::new(),
        background: None,
    });
    // Images as the editor draws them: four pixels per point plus a margin.
    for a in &scene.settings.annotations.items {
        if let Some(key) = a.shape.look_key() {
            let (w, h) = (130.0, 90.0);
            let (pw, ph) = ((w * 4.0) as u32, (h * 4.0) as u32);
            let mut rgba = vec![0u8; (pw * ph * 4) as usize];
            for px in rgba.chunks_exact_mut(4).step_by(7) {
                px.copy_from_slice(&[255, 59, 48, 255]);
            }
            scene.rasters.insert(
                key,
                Raster::new(pw, ph, rgba, (-10.0, -10.0), (w, h)).expect("raster"),
            );
        }
    }
    scene
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn main() {
    let frames: usize = std::env::args()
        .nth(1)
        .and_then(|n| n.parse().ok())
        .unwrap_or(120);
    let mut source = desktop();
    let cases = [
        ("none", 0, 0, (0.0, 0.0)),
        ("10 marks", 10, 0, (0.0, 0.0)),
        ("50 marks", 50, 0, (0.0, 0.0)),
        ("8 blur/pixelate 300×120 pt", 0, 8, (300.0, 120.0)),
        ("1 blur, whole screen", 0, 1, SCREEN),
        ("50 marks + 8 blur/pixelate", 50, 8, (300.0, 120.0)),
    ];
    let mut rows = Vec::new();
    for (size_name, size) in [("1080p", (1662, 1080)), ("4K", (3324, 2160))] {
        let mut compositor = Compositor::new(size.0, size.1, PixelFormat::Bgra8).expect("GPU");
        for zoom in [false, true] {
            for (name, marks, hidden, area) in cases {
                let scene = scene(marks, hidden, area, zoom);
                let mut times = Vec::with_capacity(frames);
                for i in 0..frames + 10 {
                    let started = Instant::now();
                    compositor
                        .render_from(&scene, &mut source, i as f64 * 16.7)
                        .expect("render");
                    compositor.read(|data, _| data[0]).expect("read");
                    if i >= 10 {
                        times.push(started.elapsed().as_secs_f64() * 1000.0);
                    }
                }
                assert_eq!(compositor.missing_rasters(), 0);
                rows.push(serde_json::json!({
                    "output": size_name,
                    "zoom": zoom,
                    "annotations": name,
                    "medianMs": (median(times.clone()) * 100.0).round() / 100.0,
                    "fps": (1000.0 / median(times) * 10.0).round() / 10.0,
                }));
            }
        }
    }
    println!("{}", serde_json::to_string_pretty(&rows).expect("json"));
}
