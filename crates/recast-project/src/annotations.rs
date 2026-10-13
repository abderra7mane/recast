use std::{collections::HashSet, fmt::Write};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{Color, Rect};

/// Size of the cells blur and pixelate average when none is given, in points.
pub const DEFAULT_BLOCK: f64 = 12.0;
pub const MAX_TRACKS: u32 = 32;

/// A position on the recorded area, in points from its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

fn default_block() -> f64 {
    DEFAULT_BLOCK
}

/// What an annotation draws. Positions and sizes are in points on the recorded area,
/// so annotations zoom and pan with the recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AnnotationShape {
    Arrow {
        from: Point,
        to: Point,
        color: Color,
        width: f64,
    },
    Line {
        from: Point,
        to: Point,
        color: Color,
        width: f64,
    },
    Rect {
        rect: Rect,
        color: Color,
        width: f64,
        fill: bool,
    },
    Ellipse {
        rect: Rect,
        color: Color,
        width: f64,
        fill: bool,
    },
    Highlight {
        rect: Rect,
        color: Color,
    },
    /// Hides the recording under it with cells `block` points wide.
    Blur {
        rect: Rect,
        #[serde(default = "default_block")]
        block: f64,
    },
    Pixelate {
        rect: Rect,
        #[serde(default = "default_block")]
        block: f64,
    },
    /// `at` is the top-left corner of the first line.
    #[serde(rename_all = "camelCase")]
    Text {
        at: Point,
        text: String,
        color: Color,
        font_size: f64,
    },
}

/// Rounds to thousandths of a point, so moved copies of a shape get the same key.
fn milli(value: f64) -> i64 {
    (value * 1000.0).round() as i64
}

fn finite(values: &[f64]) -> bool {
    values.iter().all(|v| v.is_finite())
}

fn positive_rect(rect: Rect) -> Option<Rect> {
    if !finite(&[rect.x, rect.y, rect.width, rect.height]) {
        return None;
    }
    Some(Rect {
        x: rect.x.min(rect.x + rect.width),
        y: rect.y.min(rect.y + rect.height),
        width: rect.width.abs(),
        height: rect.height.abs(),
    })
}

impl AnnotationShape {
    /// Blur and pixelate hide the recording instead of drawing on it.
    pub fn is_obscure(&self) -> bool {
        matches!(self, Self::Blur { .. } | Self::Pixelate { .. })
    }

    /// The point an image of the shape is placed from.
    pub fn anchor(&self) -> Point {
        match self {
            Self::Arrow { from, .. } | Self::Line { from, .. } => *from,
            Self::Rect { rect, .. }
            | Self::Ellipse { rect, .. }
            | Self::Highlight { rect, .. }
            | Self::Blur { rect, .. }
            | Self::Pixelate { rect, .. } => Point {
                x: rect.x,
                y: rect.y,
            },
            Self::Text { at, .. } => *at,
        }
    }

    /// What the shape looks like, wherever it is: shapes that differ only in their
    /// position share a key, and so an image. `None` for blur and pixelate, which have
    /// no image.
    pub fn look_key(&self) -> Option<String> {
        let mut key = String::new();
        let size = |key: &mut String, rect: &Rect| {
            let _ = write!(key, "{} {}", milli(rect.width), milli(rect.height));
        };
        match self {
            Self::Arrow {
                from,
                to,
                color,
                width,
            }
            | Self::Line {
                from,
                to,
                color,
                width,
            } => {
                let kind = if matches!(self, Self::Arrow { .. }) {
                    "arrow"
                } else {
                    "line"
                };
                let _ = write!(
                    key,
                    "{kind} {} {} {color} {}",
                    milli(to.x - from.x),
                    milli(to.y - from.y),
                    milli(*width)
                );
            }
            Self::Rect {
                rect,
                color,
                width,
                fill,
            }
            | Self::Ellipse {
                rect,
                color,
                width,
                fill,
            } => {
                let kind = if matches!(self, Self::Rect { .. }) {
                    "rect"
                } else {
                    "ellipse"
                };
                let _ = write!(key, "{kind} ");
                size(&mut key, rect);
                let _ = write!(key, " {color} {} {fill}", milli(*width));
            }
            Self::Highlight { rect, color } => {
                key.push_str("highlight ");
                size(&mut key, rect);
                let _ = write!(key, " {color}");
            }
            Self::Text {
                text,
                color,
                font_size,
                ..
            } => {
                let _ = write!(key, "text {color} {} {text}", milli(*font_size));
            }
            Self::Blur { .. } | Self::Pixelate { .. } => return None,
        }
        Some(key)
    }

    /// Copy with sizes in range, or `None` when a coordinate is not a number.
    fn sanitized(&self) -> Option<Self> {
        let width = |w: f64| {
            if w.is_finite() {
                w.clamp(0.5, 100.0)
            } else {
                3.0
            }
        };
        let block = |b: f64| {
            if b.is_finite() {
                b.clamp(2.0, 200.0)
            } else {
                DEFAULT_BLOCK
            }
        };
        Some(match self.clone() {
            Self::Arrow {
                from,
                to,
                color,
                width: w,
            } => {
                finite(&[from.x, from.y, to.x, to.y]).then_some(())?;
                Self::Arrow {
                    from,
                    to,
                    color,
                    width: width(w),
                }
            }
            Self::Line {
                from,
                to,
                color,
                width: w,
            } => {
                finite(&[from.x, from.y, to.x, to.y]).then_some(())?;
                Self::Line {
                    from,
                    to,
                    color,
                    width: width(w),
                }
            }
            Self::Rect {
                rect,
                color,
                width: w,
                fill,
            } => Self::Rect {
                rect: positive_rect(rect)?,
                color,
                width: width(w),
                fill,
            },
            Self::Ellipse {
                rect,
                color,
                width: w,
                fill,
            } => Self::Ellipse {
                rect: positive_rect(rect)?,
                color,
                width: width(w),
                fill,
            },
            Self::Highlight { rect, color } => Self::Highlight {
                rect: positive_rect(rect)?,
                color,
            },
            Self::Blur { rect, block: b } => Self::Blur {
                rect: positive_rect(rect)?,
                block: block(b),
            },
            Self::Pixelate { rect, block: b } => Self::Pixelate {
                rect: positive_rect(rect)?,
                block: block(b),
            },
            Self::Text {
                at,
                text,
                color,
                font_size,
            } => {
                finite(&[at.x, at.y]).then_some(())?;
                Self::Text {
                    at,
                    text,
                    color,
                    font_size: if font_size.is_finite() {
                        font_size.clamp(4.0, 400.0)
                    } else {
                        20.0
                    },
                }
            }
        })
    }
}

/// A shape shown from `start_ms` to `end_ms` on the recording's timeline, both included.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub id: String,
    pub start_ms: f64,
    pub end_ms: f64,
    /// Index of its track; later tracks are drawn on top.
    pub track: u32,
    pub shape: AnnotationShape,
}

impl Annotation {
    pub fn visible_at(&self, t_ms: f64) -> bool {
        self.start_ms <= t_ms && t_ms <= self.end_ms
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Annotations {
    /// How many tracks the timeline shows, empty ones included.
    pub tracks: u32,
    /// Sorted by track, then by start.
    pub items: Vec<Annotation>,
}

impl Default for Annotations {
    fn default() -> Self {
        Self {
            tracks: 1,
            items: Vec::new(),
        }
    }
}

impl Annotations {
    /// Copy with valid items inside `duration_ms`, unique ids and every track counted.
    pub fn sanitized(&self, duration_ms: f64) -> Self {
        let duration = duration_ms.max(0.0);
        let mut ids = HashSet::new();
        let tracks = self.tracks.clamp(1, MAX_TRACKS);
        let mut items: Vec<Annotation> = self
            .items
            .iter()
            .filter(|a| finite(&[a.start_ms, a.end_ms]))
            .filter_map(|a| {
                let start_ms = a.start_ms.clamp(0.0, duration);
                let end_ms = a.end_ms.clamp(0.0, duration);
                (end_ms > start_ms && ids.insert(a.id.clone())).then_some(())?;
                Some(Annotation {
                    id: a.id.clone(),
                    start_ms,
                    end_ms,
                    track: a.track.min(MAX_TRACKS - 1),
                    shape: a.shape.sanitized()?,
                })
            })
            .collect();
        items.sort_by(|a, b| {
            a.track
                .cmp(&b.track)
                .then(a.start_ms.total_cmp(&b.start_ms))
        });
        let used = items.iter().map(|a| a.track + 1).max().unwrap_or(1);
        Self {
            tracks: tracks.max(used),
            items,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn annotation(id: &str, start_ms: f64, end_ms: f64, track: u32) -> Annotation {
        Annotation {
            id: id.into(),
            start_ms,
            end_ms,
            track,
            shape: AnnotationShape::Rect {
                rect: rect(10.0, 20.0, 100.0, 50.0),
                color: Color::rgb(255, 59, 48),
                width: 3.0,
                fill: false,
            },
        }
    }

    #[test]
    fn json_matches_the_markup_shapes() {
        let arrow = AnnotationShape::Arrow {
            from: Point { x: 1.0, y: 2.0 },
            to: Point { x: 3.0, y: 4.0 },
            color: Color::rgb(255, 0, 0),
            width: 3.0,
        };
        let json = serde_json::to_value(&arrow).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "kind": "arrow",
                "from": {"x": 1.0, "y": 2.0},
                "to": {"x": 3.0, "y": 4.0},
                "color": "#ff0000",
                "width": 3.0,
            })
        );
        let text: AnnotationShape = serde_json::from_value(serde_json::json!({
            "kind": "text",
            "at": {"x": 5, "y": 6},
            "text": "Hi",
            "color": "#ffffff",
            "fontSize": 20,
        }))
        .unwrap();
        assert!(matches!(text, AnnotationShape::Text { font_size, .. } if font_size == 20.0));
        let blur: AnnotationShape = serde_json::from_value(serde_json::json!({
            "kind": "blur",
            "rect": {"x": 0, "y": 0, "width": 10, "height": 10},
        }))
        .unwrap();
        assert!(matches!(blur, AnnotationShape::Blur { block, .. } if block == DEFAULT_BLOCK));
    }

    #[test]
    fn settings_without_annotations_get_one_empty_track() {
        let settings: crate::EditSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.annotations, Annotations::default());
        assert_eq!(settings.annotations.tracks, 1);
    }

    #[test]
    fn moved_shapes_share_a_key_and_changed_ones_do_not() {
        let base = annotation("a", 0.0, 1.0, 0).shape;
        let AnnotationShape::Rect {
            rect: r,
            color,
            width,
            fill,
        } = base.clone()
        else {
            unreachable!()
        };
        let moved = AnnotationShape::Rect {
            rect: rect(r.x + 0.1 + 0.2, r.y - 17.3, r.width, r.height),
            color,
            width,
            fill,
        };
        assert_eq!(base.look_key(), moved.look_key());
        let wider = AnnotationShape::Rect {
            rect: rect(r.x, r.y, r.width + 1.0, r.height),
            color,
            width,
            fill,
        };
        assert_ne!(base.look_key(), wider.look_key());
        let ellipse = AnnotationShape::Ellipse {
            rect: r,
            color,
            width,
            fill,
        };
        assert_ne!(base.look_key(), ellipse.look_key());

        let line = |kind: &str, dx: f64| {
            let (from, to) = (
                Point { x: 5.0, y: 5.0 },
                Point {
                    x: 5.0 + dx,
                    y: 9.0,
                },
            );
            let color = Color::rgb(0, 0, 0);
            if kind == "arrow" {
                AnnotationShape::Arrow {
                    from,
                    to,
                    color,
                    width: 2.0,
                }
            } else {
                AnnotationShape::Line {
                    from,
                    to,
                    color,
                    width: 2.0,
                }
            }
        };
        assert_ne!(line("arrow", 3.0).look_key(), line("line", 3.0).look_key());
        assert_ne!(line("arrow", 3.0).look_key(), line("arrow", 4.0).look_key());
        let blur = AnnotationShape::Blur {
            rect: r,
            block: 12.0,
        };
        assert_eq!(blur.look_key(), None);
        assert!(blur.is_obscure() && !base.is_obscure());
    }

    #[test]
    fn text_keys_include_the_text() {
        let text = |text: &str| AnnotationShape::Text {
            at: Point { x: 0.0, y: 0.0 },
            text: text.into(),
            color: Color::rgb(1, 2, 3),
            font_size: 20.0,
        };
        assert_ne!(text("a").look_key(), text("b").look_key());
        assert_eq!(text("é ✓").look_key(), text("é ✓").look_key());
    }

    #[test]
    fn sanitize_clamps_drops_and_sorts() {
        let mut flipped = annotation("flipped", 900.0, 1_500.0, 0);
        flipped.shape = AnnotationShape::Blur {
            rect: rect(50.0, 40.0, -30.0, -20.0),
            block: 0.0,
        };
        let mut broken = annotation("broken", 0.0, 100.0, 0);
        broken.shape = AnnotationShape::Line {
            from: Point {
                x: f64::NAN,
                y: 0.0,
            },
            to: Point { x: 1.0, y: 1.0 },
            color: Color::rgb(0, 0, 0),
            width: 2.0,
        };
        let annotations = Annotations {
            tracks: 0,
            items: vec![
                annotation("late", 3_000.0, 9_000.0, 2),
                flipped,
                annotation("empty", 500.0, 500.0, 0),
                annotation("late", 0.0, 100.0, 0),
                annotation("early", 100.0, 400.0, 0),
                broken,
                annotation("nan", f64::NAN, 100.0, 0),
            ],
        };
        let clean = annotations.sanitized(5_000.0);
        let ids: Vec<_> = clean.items.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["early", "flipped", "late"]);
        assert_eq!(clean.tracks, 3);
        assert_eq!(clean.items[2].end_ms, 5_000.0);
        assert_eq!(
            clean.items[1].shape,
            AnnotationShape::Blur {
                rect: rect(20.0, 20.0, 30.0, 20.0),
                block: 2.0
            }
        );
    }

    #[test]
    fn visibility_includes_both_ends() {
        let a = annotation("a", 100.0, 200.0, 0);
        assert!(!a.visible_at(99.9));
        assert!(a.visible_at(100.0));
        assert!(a.visible_at(200.0));
        assert!(!a.visible_at(200.1));
    }
}
