use cidre::{cg, cv, sc};

use super::{block_on, content, writer::platform};
use crate::{CaptureTarget, Error, Result, Screenshot};

pub(super) fn screenshot(target: &CaptureTarget) -> Result<Screenshot> {
    let content = content::shareable_content()?;
    let source = content::source(&content, target)?;
    let width = (source.width * source.scale).round().max(1.0) as usize;
    let height = (source.height * source.scale).round().max(1.0) as usize;

    let mut cfg = sc::StreamCfg::new();
    cfg.set_width(width);
    cfg.set_height(height);
    cfg.set_shows_cursor(false);
    cfg.set_capture_resolution(sc::CaptureResolution::Best);
    cfg.set_pixel_format(cv::PixelFormat::_32_BGRA);
    cfg.set_color_space_name(cg::color_space::names::srgb());
    if let Some(rect) = source.src_rect {
        cfg.set_src_rect(rect);
    }
    if source.window.is_some() {
        cfg.set_ignore_shadows_single_window(true);
        cfg.set_should_be_opaque(false);
        cfg.set_background_color(&cg::Color::generic_gray(0.0, 0.0));
    }

    let image =
        block_on(sc::ScreenshotManager::capture_image(&source.filter, &cfg)).map_err(|e| {
            if cg::screen_capture_access::preflight() {
                platform("cannot take the screenshot", &e)
            } else {
                Error::PermissionDenied
            }
        })?;
    let (width, height, rgba) = image_rgba(&image)?;
    Ok(Screenshot {
        width,
        height,
        scale_factor: source.scale,
        rgba,
    })
}

/// Draws `image` into an sRGB bitmap and returns its size and straight RGBA pixels.
pub fn image_rgba(image: &cg::Image) -> Result<(u32, u32, Vec<u8>)> {
    let (width, height) = (image.width(), image.height());
    let space = cg::ColorSpace::with_name(cg::color_space::names::srgb())
        .ok_or_else(|| Error::Platform("no sRGB color space".into()))?;
    let mut pixels = vec![0u8; width * height * 4];
    {
        let mut context = cg::Context::new_bitmap(
            pixels.as_mut_ptr().cast(),
            width,
            height,
            8,
            width * 4,
            &space,
            cg::BitmapInfo::with_alpha(cg::ImageAlphaInfo::PremultipliedLast),
        )
        .ok_or_else(|| Error::Platform(format!("cannot draw a {width}×{height} image")))?;
        context.draw_image(
            cg::Rect {
                origin: cg::Point { x: 0.0, y: 0.0 },
                size: cg::Size {
                    width: width as f64,
                    height: height as f64,
                },
            },
            image,
        );
    }
    unpremultiply(&mut pixels);
    Ok((width as u32, height as u32, pixels))
}

fn unpremultiply(pixels: &mut [u8]) {
    for px in pixels.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a == 0 || a == 255 {
            continue;
        }
        for c in &mut px[..3] {
            *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3×2 image: the top row opaque blue, the bottom row half-transparent red.
    fn synthetic_image() -> cidre::arc::R<cg::Image> {
        let space = cg::ColorSpace::with_name(cg::color_space::names::srgb()).unwrap();
        let mut context = cg::Context::new_bitmap(
            std::ptr::null_mut(),
            3,
            2,
            8,
            12,
            &space,
            cg::BitmapInfo::with_alpha(cg::ImageAlphaInfo::PremultipliedLast),
        )
        .unwrap();
        let row = |y: f64| cg::Rect {
            origin: cg::Point { x: 0.0, y },
            size: cg::Size {
                width: 3.0,
                height: 1.0,
            },
        };
        context.set_rgb_fill_color(0.0, 0.0, 1.0, 1.0);
        context.fill_rect(row(1.0));
        context.set_rgb_fill_color(1.0, 0.0, 0.0, 0.5);
        context.fill_rect(row(0.0));
        context.bitmap_image().unwrap()
    }

    #[test]
    fn image_pixels_are_straight_rgba_top_row_first() {
        let (width, height, pixels) = image_rgba(&synthetic_image()).unwrap();
        assert_eq!((width, height), (3, 2));
        assert_eq!(&pixels[0..4], &[0, 0, 255, 255]);
        let bottom = &pixels[12..16];
        assert!(
            bottom[0] >= 253 && bottom[1] == 0 && bottom[2] == 0,
            "{bottom:?}"
        );
        assert!((127..=128).contains(&bottom[3]), "{bottom:?}");
    }

    #[test]
    fn unpremultiply_restores_color() {
        let mut px = [64, 32, 0, 128, 10, 20, 30, 255, 0, 0, 0, 0];
        unpremultiply(&mut px);
        assert_eq!(px, [128, 64, 0, 128, 10, 20, 30, 255, 0, 0, 0, 0]);
    }
}
