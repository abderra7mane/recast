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

/// `padding` (a fraction of the shorter side) in whole pixels for a `width × height` image.
pub fn padding_pixels(width: u32, height: u32, padding: f64) -> u32 {
    (padding.clamp(0.0, 0.5) * width.min(height) as f64).round() as u32
}

/// Frame size that shows a `width × height` pixel image at its own size, with `padding`
/// (a fraction of its shorter side, rounded to whole pixels) around it.
pub fn native_size(width: u32, height: u32, padding: f64) -> (u32, u32) {
    let pad = padding_pixels(width, height, padding);
    (width + 2 * pad, height + 2 * pad)
}

/// Output size for a `video_width × video_height` pixel recording with `padding` around
/// it. `Auto` shows every video pixel as one output pixel; a preset sets the shorter side
/// and keeps the aspect ratio of the padded video. Sizes are even, as 4:2:0 video needs.
pub fn output_size(
    video_width: u32,
    video_height: u32,
    padding: f64,
    resolution: Resolution,
) -> (u32, u32) {
    let Some(short) = resolution.short_side() else {
        let (w, h) = native_size(video_width, video_height, padding);
        return (w + w % 2, h + h % 2);
    };
    let (uw, uh) = unit_size(video_width as f64, video_height as f64);
    let (cw, ch) = (uw + 2.0 * padding, uh + 2.0 * padding);
    let short = short as f64;
    if cw >= ch {
        (even(short * cw / ch), even(short))
    } else {
        (even(short), even(short * ch / cw))
    }
}

/// How much a frame of `width × height` (from [`output_size`]) enlarges the video when
/// nothing is zoomed in; 1 at `Auto`.
pub fn video_scale(
    width: u32,
    height: u32,
    video_width: u32,
    video_height: u32,
    background: &BackgroundSettings,
) -> f64 {
    layout(width, height, video_width, video_height, background)
        .content
        .width
        / video_width.max(1) as f64
}

/// Fits the padded video into a `width × height` frame, centered. The padding is
/// rounded to whole video pixels, so a frame of [`native_size`] shows the video 1:1.
pub fn layout(
    width: u32,
    height: u32,
    video_width: u32,
    video_height: u32,
    background: &BackgroundSettings,
) -> Layout {
    let (uw, uh) = unit_size(video_width as f64, video_height as f64);
    let p = padding_pixels(video_width, video_height, background.padding) as f64
        / video_width.min(video_height).max(1) as f64;
    let unit = (width as f64 / (uw + 2.0 * p)).min(height as f64 / (uh + 2.0 * p));
    let (cw, ch) = (uw * unit, uh * unit);
    // On whole pixels, so an odd leftover pixel does not put the video between two.
    let origin = |free: f64| (free / 2.0 + 1e-6).floor();
    Layout {
        width,
        height,
        content: PixelRect {
            x: origin(width as f64 - cw),
            y: origin(height as f64 - ch),
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
            output_size(1920, 1080, 0.0, Resolution::P1080),
            (1920, 1080)
        );
        assert_eq!(
            output_size(3840, 2160, 0.0, Resolution::P2160),
            (3840, 2160)
        );
    }

    #[test]
    fn padding_widens_the_frame_and_keeps_sizes_even() {
        let (w, h) = output_size(1512, 982, 0.08, Resolution::P1080);
        assert_eq!(h, 1080);
        assert_eq!(w % 2, 0);
        let expected = 1080.0 * (1512.0 / 982.0 + 0.16) / 1.16;
        assert!((w as f64 - expected).abs() <= 2.0, "{w} vs {expected}");
    }

    #[test]
    fn portrait_screens_use_the_width() {
        let (w, h) = output_size(800, 1200, 0.0, Resolution::P1440);
        assert_eq!((w, h), (1440, 2160));
    }

    #[test]
    fn auto_keeps_the_video_pixels_and_pads_with_whole_pixels() {
        let bg = BackgroundSettings {
            padding: 0.08,
            ..Default::default()
        };
        let (w, h) = output_size(1368, 954, bg.padding, Resolution::Auto);
        assert_eq!((w, h), (1368 + 2 * 76, 954 + 2 * 76));
        let l = layout(w, h, 1368, 954, &bg);
        assert_eq!((l.content.x, l.content.y), (76.0, 76.0));
        assert!((l.content.width - 1368.0).abs() < 1e-9);
        assert!((l.content.height - 954.0).abs() < 1e-9);
        assert!((video_scale(w, h, 1368, 954, &bg) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn auto_makes_odd_videos_even_without_moving_the_video_off_whole_pixels() {
        let bg = BackgroundSettings {
            padding: 0.1,
            ..Default::default()
        };
        let (w, h) = output_size(301, 200, bg.padding, Resolution::Auto);
        assert_eq!((w, h), (342, 240));
        let l = layout(w, h, 301, 200, &bg);
        assert_eq!((l.content.x, l.content.y), (20.0, 20.0));
        assert!((l.content.width - 301.0).abs() < 1e-9);
    }

    #[test]
    fn presets_report_how_much_they_enlarge() {
        let bg = BackgroundSettings {
            padding: 0.08,
            ..Default::default()
        };
        let (w, h) = output_size(1368, 954, bg.padding, Resolution::P2160);
        assert_eq!(h, 2160);
        let scale = video_scale(w, h, 1368, 954, &bg);
        assert!(
            (scale - 2160.0 / (954.0 + 2.0 * 76.0)).abs() < 1e-3,
            "{scale}"
        );
    }

    #[test]
    fn content_is_centered_with_padding() {
        let mut bg = BackgroundSettings {
            padding: 0.1,
            ..Default::default()
        };
        bg.corner_radius = 0.05;
        let l = layout(1100, 600, 1000, 500, &bg);
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(close(l.content.width, 1000.0) && close(l.content.height, 500.0));
        assert!(close(l.content.x, 50.0) && close(l.content.y, 50.0));
        assert!(close(l.corner_radius, 25.0));
    }
}
