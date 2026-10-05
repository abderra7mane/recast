use recast_project::{BackgroundSettings, Resolution};

/// A rectangle in output pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Where the screen sits in the output frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub width: u32,
    pub height: u32,
    pub content: PixelRect,
    pub corner_radius: f64,
    pub shadow_blur: f64,
    pub shadow_offset_y: f64,
}

/// Screen size in units of its shorter side, e.g. 1.6 × 1 for 16:10.
fn unit_size(screen_width: f64, screen_height: f64) -> (f64, f64) {
    let aspect = screen_width / screen_height.max(1e-9);
    if aspect >= 1.0 {
        (aspect, 1.0)
    } else {
        (1.0, 1.0 / aspect)
    }
}

fn even(value: f64) -> u32 {
    let v = value.round().max(2.0) as u32;
    v - v % 2
}

/// Output size for a screen of `screen_width × screen_height` (any unit) with
/// `padding` around it: the shorter side is the resolution preset and the aspect
/// ratio is the screen's plus padding.
pub fn output_size(
    screen_width: f64,
    screen_height: f64,
    padding: f64,
    resolution: Resolution,
) -> (u32, u32) {
    let (uw, uh) = unit_size(screen_width, screen_height);
    let (cw, ch) = (uw + 2.0 * padding, uh + 2.0 * padding);
    let short = resolution.short_side() as f64;
    if cw >= ch {
        (even(short * cw / ch), even(short))
    } else {
        (even(short), even(short * ch / cw))
    }
}

/// Fits the padded screen into a `width × height` frame, centered.
pub fn layout(
    width: u32,
    height: u32,
    screen_width: f64,
    screen_height: f64,
    background: &BackgroundSettings,
) -> Layout {
    let (uw, uh) = unit_size(screen_width, screen_height);
    let p = background.padding;
    let unit = (width as f64 / (uw + 2.0 * p)).min(height as f64 / (uh + 2.0 * p));
    let (cw, ch) = (uw * unit, uh * unit);
    Layout {
        width,
        height,
        content: PixelRect {
            x: (width as f64 - cw) / 2.0,
            y: (height as f64 - ch) / 2.0,
            width: cw,
            height: ch,
        },
        corner_radius: (background.corner_radius * unit).min(cw.min(ch) / 2.0),
        shadow_blur: background.shadow.blur * unit,
        shadow_offset_y: background.shadow.offset_y * unit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixteen_by_nine_without_padding_is_exact() {
        assert_eq!(
            output_size(1920.0, 1080.0, 0.0, Resolution::P1080),
            (1920, 1080)
        );
        assert_eq!(
            output_size(3840.0, 2160.0, 0.0, Resolution::P2160),
            (3840, 2160)
        );
    }

    #[test]
    fn padding_widens_the_frame_and_keeps_sizes_even() {
        let (w, h) = output_size(1512.0, 982.0, 0.08, Resolution::P1080);
        assert_eq!(h, 1080);
        assert_eq!(w % 2, 0);
        let expected = 1080.0 * (1512.0 / 982.0 + 0.16) / 1.16;
        assert!((w as f64 - expected).abs() <= 2.0, "{w} vs {expected}");
    }

    #[test]
    fn portrait_screens_use_the_width() {
        let (w, h) = output_size(800.0, 1200.0, 0.0, Resolution::P1440);
        assert_eq!((w, h), (1440, 2160));
    }

    #[test]
    fn content_is_centered_with_padding() {
        let mut bg = BackgroundSettings {
            padding: 0.1,
            ..Default::default()
        };
        bg.corner_radius = 0.05;
        let l = layout(1100, 600, 1000.0, 500.0, &bg);
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(close(l.content.width, 1000.0) && close(l.content.height, 500.0));
        assert!(close(l.content.x, 50.0) && close(l.content.y, 50.0));
        assert!(close(l.corner_radius, 25.0));
    }
}
