use crate::bitmap::Rgba;

/// A cursor bitmap with its size and hotspot in points.
#[derive(Debug, Clone, PartialEq)]
pub struct CursorImage {
    pub image: Rgba,
    pub width_pts: f64,
    pub height_pts: f64,
    pub hotspot_x: f64,
    pub hotspot_y: f64,
}

/// Outline of a standard arrow, tip at the origin, in points.
const ARROW: [(f64, f64); 7] = [
    (0.0, 0.0),
    (0.0, 17.0),
    (4.0, 13.0),
    (7.0, 19.5),
    (9.6, 18.4),
    (6.7, 12.2),
    (12.0, 12.2),
];
const OUTLINE: f64 = 1.25;
const MARGIN: f64 = OUTLINE + 0.75;

fn distance_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let (apx, apy) = (p.0 - a.0, p.1 - a.1);
    let t = ((apx * abx + apy * aby) / (abx * abx + aby * aby)).clamp(0.0, 1.0);
    ((apx - t * abx).powi(2) + (apy - t * aby).powi(2)).sqrt()
}

/// Signed distance to the arrow outline; negative inside.
fn arrow_distance(p: (f64, f64)) -> f64 {
    let mut distance = f64::MAX;
    let mut inside = false;
    for i in 0..ARROW.len() {
        let a = ARROW[i];
        let b = ARROW[(i + 1) % ARROW.len()];
        distance = distance.min(distance_to_segment(p, a, b));
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
    }
    if inside { -distance } else { distance }
}

/// The cursor drawn when a recording has no cursor shape: a black arrow with a
/// white outline, rendered at `scale` pixels per point.
pub fn default_arrow(scale: f64) -> CursorImage {
    let width_pts = 12.0 + 2.0 * MARGIN;
    let height_pts = 19.5 + 2.0 * MARGIN;
    let width = (width_pts * scale).ceil() as u32;
    let height = (height_pts * scale).ceil() as u32;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    let edge = 0.5 / scale;
    for y in 0..height {
        for x in 0..width {
            let p = (
                (x as f64 + 0.5) / scale - MARGIN,
                (y as f64 + 0.5) / scale - MARGIN,
            );
            let d = arrow_distance(p);
            let coverage = ((OUTLINE - d) / (2.0 * edge) + 0.5).clamp(0.0, 1.0);
            let black = ((-d) / (2.0 * edge) + 0.5).clamp(0.0, 1.0);
            let value = (255.0 * (1.0 - black)).round() as u8;
            let alpha = (255.0 * coverage).round() as u8;
            pixels.extend_from_slice(&[value, value, value, alpha]);
        }
    }
    CursorImage {
        image: Rgba::from_straight(width, height, pixels),
        width_pts: width as f64 / scale,
        height_pts: height as f64 / scale,
        hotspot_x: MARGIN,
        hotspot_y: MARGIN,
    }
}

/// The default arrow as straight RGBA, e.g. for writing a cursor PNG.
pub fn default_arrow_straight(scale: f64) -> (u32, u32, Vec<u8>, f64, f64) {
    let arrow = default_arrow(scale);
    let mut pixels = arrow.image.pixels.clone();
    for px in pixels.chunks_exact_mut(4) {
        let a = px[3] as u32;
        for c in &mut px[..3] {
            if let Some(v) = (*c as u32 * 255 + a / 2).checked_div(a) {
                *c = v.min(255) as u8;
            }
        }
    }
    (
        arrow.image.width,
        arrow.image.height,
        pixels,
        arrow.hotspot_x,
        arrow.hotspot_y,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrow_is_black_inside_white_outline_transparent_outside() {
        let arrow = default_arrow(4.0);
        let px = |x_pts: f64, y_pts: f64| {
            let x = ((x_pts + MARGIN) * 4.0) as u32;
            let y = ((y_pts + MARGIN) * 4.0) as u32;
            let i = ((y * arrow.image.width + x) * 4) as usize;
            arrow.image.pixels[i..i + 4].to_vec()
        };
        assert_eq!(px(2.0, 8.0), vec![0, 0, 0, 255]);
        assert_eq!(px(-0.6, 8.0), vec![255, 255, 255, 255]);
        assert_eq!(px(11.0, 2.0)[3], 0);
        assert_eq!(arrow.hotspot_x, MARGIN);
    }
}
