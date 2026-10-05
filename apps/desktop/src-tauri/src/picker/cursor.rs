//! The picker's camera cursor, drawn in code: a black camera with a white outline whose
//! lens center is the hotspot.

pub const SIZE_POINTS: f64 = 24.0;
pub const HOTSPOT: (f64, f64) = (12.0, 13.0);

const OUTLINE: f64 = 1.5;
const LENS_RADIUS: f64 = 3.6;
const LENS_RING: f64 = 1.3;

fn sd_round_rect(px: f64, py: f64, x0: f64, y0: f64, x1: f64, y1: f64, radius: f64) -> f64 {
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (hw, hh) = ((x1 - x0) / 2.0 - radius, (y1 - y0) / 2.0 - radius);
    let qx = (px - cx).abs() - hw;
    let qy = (py - cy).abs() - hh;
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
}

/// Premultiplied RGBA of the cursor at `scale` pixels per point, with its size in pixels.
pub fn camera_pixels(scale: f64) -> (usize, Vec<u8>) {
    let size = (SIZE_POINTS * scale).round() as usize;
    let mut pixels = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let px = (x as f64 + 0.5) / scale;
            let py = (y as f64 + 0.5) / scale;
            let body = sd_round_rect(px, py, 3.0, 8.0, 21.0, 19.0, 2.5);
            let bump = sd_round_rect(px, py, 8.0, 5.5, 15.0, 9.0, 1.2);
            let shape = body.min(bump);
            let lens = (px - HOTSPOT.0).hypot(py - HOTSPOT.1);
            let cover = |distance: f64| (0.5 - distance * scale).clamp(0.0, 1.0);
            let alpha = cover(shape - OUTLINE);
            let outline = 1.0 - cover(shape);
            let ring = cover((lens - LENS_RADIUS).abs() - LENS_RING / 2.0);
            let white = outline.max(ring) * alpha;
            let value = (white * 255.0).round() as u8;
            pixels.extend_from_slice(&[value, value, value, (alpha * 255.0).round() as u8]);
        }
    }
    (size, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_has_a_dark_lens_center_in_a_white_ring() {
        let (size, pixels) = camera_pixels(2.0);
        assert_eq!(size, 48);
        let at = |x: f64, y: f64| {
            let i = (((y * 2.0) as usize) * size + (x * 2.0) as usize) * 4;
            [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
        };
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
}
