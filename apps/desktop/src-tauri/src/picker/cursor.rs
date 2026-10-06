//! The picker's cursors, drawn in code, and the placement of the label next to the
//! crosshair.
//!
//! The camera is black with a white outline; its lens center is the hotspot. The
//! crosshair is a thin black plus around a small ring, outlined in white like the camera
//! so it shows on any background; its center is the hotspot.

pub const SIZE_POINTS: f64 = 24.0;
pub const HOTSPOT: (f64, f64) = (12.0, 13.0);
pub const CROSSHAIR_HOTSPOT: (f64, f64) = (12.0, 12.0);

const OUTLINE: f64 = 1.5;
const LENS_RADIUS: f64 = 3.6;
const LENS_RING: f64 = 1.3;

const CROSSHAIR_OUTLINE: f64 = 1.0;
const CROSSHAIR_LINE: f64 = 1.0;
const CROSSHAIR_RING: f64 = 2.5;
const CROSSHAIR_ARM: (f64, f64) = (5.0, 11.0);

/// Gap between the crosshair's center and its label.
const LABEL_OFFSET: f64 = 14.0;
/// The label keeps this far from the display's edges.
const LABEL_EDGE: f64 = 4.0;

fn sd_round_rect(px: f64, py: f64, x0: f64, y0: f64, x1: f64, y1: f64, radius: f64) -> f64 {
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (hw, hh) = ((x1 - x0) / 2.0 - radius, (y1 - y0) / 2.0 - radius);
    let qx = (px - cx).abs() - hw;
    let qy = (py - cy).abs() - hh;
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
}

/// Premultiplied RGBA of a black `shape` (a signed distance in points, negative inside)
/// with a white outline `outline` points wide, `size` points square at `scale`.
fn outlined(scale: f64, outline: f64, shape: impl Fn(f64, f64) -> (f64, f64)) -> (usize, Vec<u8>) {
    let size = (SIZE_POINTS * scale).round() as usize;
    let mut pixels = Vec::with_capacity(size * size * 4);
    let cover = |distance: f64| (0.5 - distance * scale).clamp(0.0, 1.0);
    for y in 0..size {
        for x in 0..size {
            let (body, white_mark) = shape((x as f64 + 0.5) / scale, (y as f64 + 0.5) / scale);
            let alpha = cover(body - outline);
            let white = (1.0 - cover(body)).max(cover(white_mark)) * alpha;
            let value = (white * 255.0).round() as u8;
            pixels.extend_from_slice(&[value, value, value, (alpha * 255.0).round() as u8]);
        }
    }
    (size, pixels)
}

/// The camera at `scale` pixels per point, with its size in pixels.
pub fn camera_pixels(scale: f64) -> (usize, Vec<u8>) {
    outlined(scale, OUTLINE, |px, py| {
        let body = sd_round_rect(px, py, 3.0, 8.0, 21.0, 19.0, 2.5);
        let bump = sd_round_rect(px, py, 8.0, 5.5, 15.0, 9.0, 1.2);
        let lens = (px - HOTSPOT.0).hypot(py - HOTSPOT.1);
        let ring = (lens - LENS_RADIUS).abs() - LENS_RING / 2.0;
        (body.min(bump), ring)
    })
}

/// The crosshair at `scale` pixels per point, with its size in pixels.
pub fn crosshair_pixels(scale: f64) -> (usize, Vec<u8>) {
    let (cx, cy) = CROSSHAIR_HOTSPOT;
    let half = CROSSHAIR_LINE / 2.0;
    let (near, far) = CROSSHAIR_ARM;
    outlined(scale, CROSSHAIR_OUTLINE, |px, py| {
        let (dx, dy) = ((px - cx).abs(), (py - cy).abs());
        let horizontal = sd_round_rect(dx, dy, near, -half, far, half, 0.0);
        let vertical = sd_round_rect(dx, dy, -half, near, half, far, 0.0);
        let ring = (dx.hypot(dy) - CROSSHAIR_RING).abs() - half;
        (horizontal.min(vertical).min(ring), f64::INFINITY)
    })
}

/// Top-left corner of the crosshair's label in display points (y down): below and to
/// the right of the pointer, or on the other side of it when the label would leave the
/// display.
pub fn label_origin(pointer: (f64, f64), label: (f64, f64), display: (f64, f64)) -> (f64, f64) {
    let place = |at: f64, size: f64, room: f64| {
        let after = at + LABEL_OFFSET;
        let start = if after + size + LABEL_EDGE <= room {
            after
        } else {
            at - LABEL_OFFSET - size
        };
        start.clamp(LABEL_EDGE, (room - size - LABEL_EDGE).max(LABEL_EDGE))
    };
    (
        place(pointer.0, label.0, display.0),
        place(pointer.1, label.1, display.1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(pixels: &[u8], size: usize, scale: f64, x: f64, y: f64) -> [u8; 4] {
        let i = (((y * scale) as usize) * size + (x * scale) as usize) * 4;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    }

    #[test]
    fn camera_has_a_dark_lens_center_in_a_white_ring() {
        let (size, pixels) = camera_pixels(2.0);
        assert_eq!(size, 48);
        let at = |x, y| pixel(&pixels, size, 2.0, x, y);
        assert_eq!(at(HOTSPOT.0, HOTSPOT.1), [0, 0, 0, 255], "hotspot");
        assert_eq!(
            at(HOTSPOT.0 + LENS_RADIUS, HOTSPOT.1),
            [255, 255, 255, 255],
            "ring"
        );
        assert_eq!(at(5.0, 17.0), [0, 0, 0, 255], "body");
        assert_eq!(at(2.0, 13.0)[3], 255, "outline");
        assert_eq!(at(2.0, 13.0)[0], 255, "outline is white");
        assert_eq!(at(0.5, 0.5), [0, 0, 0, 0], "outside");
    }

    #[test]
    fn crosshair_is_a_black_plus_around_a_clear_center() {
        let (size, pixels) = crosshair_pixels(2.0);
        assert_eq!(size, 48);
        let at = |x, y| pixel(&pixels, size, 2.0, x, y);
        let (cx, cy) = CROSSHAIR_HOTSPOT;
        assert_eq!(at(cx, cy), [0, 0, 0, 0], "see-through center");
        assert_eq!(at(cx + 2.0, cy), [0, 0, 0, 255], "ring");
        assert_eq!(at(cx - CROSSHAIR_RING, cy - 0.5), [0, 0, 0, 255], "ring");
        for (x, y) in [
            (cx + 8.0, cy),
            (cx - 8.0, cy - 0.5),
            (cx, cy + 8.0),
            (cx - 0.5, cy - 8.0),
        ] {
            assert_eq!(at(x, y), [0, 0, 0, 255], "arm at {x},{y}");
        }
        assert_eq!(
            at(cx + 8.0, cy + 1.0),
            [255, 255, 255, 255],
            "white outline"
        );
        assert_eq!(at(cx + 8.0, cy + 3.0), [0, 0, 0, 0], "beside an arm");
        assert_eq!(at(0.5, 0.5), [0, 0, 0, 0], "corner");
    }

    #[test]
    fn label_sits_below_right_of_the_pointer() {
        let display = (1512.0, 982.0);
        assert_eq!(
            label_origin((100.0, 200.0), (80.0, 22.0), display),
            (114.0, 214.0)
        );
    }

    #[test]
    fn label_flips_near_the_right_and_bottom_edges() {
        let display = (1512.0, 982.0);
        let label = (80.0, 22.0);
        assert_eq!(
            label_origin((1450.0, 200.0), label, display),
            (1356.0, 214.0),
            "left of the pointer"
        );
        assert_eq!(
            label_origin((100.0, 960.0), label, display),
            (114.0, 924.0),
            "above the pointer"
        );
        assert_eq!(
            label_origin((1511.0, 981.0), label, display),
            (1417.0, 945.0),
            "bottom-right corner"
        );
        assert_eq!(
            label_origin((1394.0, 942.0), label, display),
            (1408.0, 956.0),
            "just fits"
        );
    }

    #[test]
    fn label_stays_on_tiny_displays() {
        assert_eq!(
            label_origin((5.0, 5.0), (80.0, 22.0), (60.0, 20.0)),
            (4.0, 4.0)
        );
        let (x, y) = label_origin((0.0, 0.0), (80.0, 22.0), (1512.0, 982.0));
        assert!(x >= 4.0 && y >= 4.0);
    }
}
