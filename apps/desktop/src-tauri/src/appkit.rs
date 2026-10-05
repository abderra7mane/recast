//! Small AppKit helpers shared by the picker overlay and the screenshot thumbnail.

use dispatch2::{DispatchQueue, MainThreadBound};
use objc2::{AllocAnyThread, MainThreadMarker, rc::Retained, runtime::AnyObject};
use objc2_app_kit::{
    NSBezierPath, NSBitmapImageRep, NSColor, NSCursor, NSDeviceRGBColorSpace, NSFont,
    NSFontAttributeName, NSFontWeightSemibold, NSForegroundColorAttributeName, NSImage, NSScreen,
    NSStringDrawing,
};
use objc2_foundation::{NSDictionary, NSNumber, NSPoint, NSRect, NSSize, NSString};
use recast_project::Rect;

pub fn ns_rect(r: &Rect) -> NSRect {
    NSRect::new(NSPoint::new(r.x, r.y), NSSize::new(r.width, r.height))
}

pub fn rect(r: NSRect) -> Rect {
    Rect {
        x: r.origin.x,
        y: r.origin.y,
        width: r.size.width,
        height: r.size.height,
    }
}

/// `r` shrunk by `dx` and `dy` on each side; negative values grow it.
pub fn inset(r: NSRect, dx: f64, dy: f64) -> NSRect {
    NSRect::new(
        NSPoint::new(r.origin.x + dx, r.origin.y + dy),
        NSSize::new(r.size.width - 2.0 * dx, r.size.height - 2.0 * dy),
    )
}

pub fn color(r: f64, g: f64, b: f64, a: f64) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, a)
}

pub fn fill(rect: NSRect, color: &NSColor) {
    color.setFill();
    NSBezierPath::fillRect(rect);
}

pub fn fill_rounded(rect: NSRect, radius: f64, color: &NSColor) {
    color.setFill();
    NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect, radius, radius).fill();
}

pub fn stroke_rounded(rect: NSRect, radius: f64, width: f64, color: &NSColor) {
    color.setStroke();
    let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect, radius, radius);
    path.setLineWidth(width);
    path.stroke();
}

pub struct TextStyle(Retained<NSDictionary<NSString, AnyObject>>);

impl TextStyle {
    pub fn new(size: f64, color: &NSColor) -> Self {
        // SAFETY: the weight constant is a plain float exported by AppKit.
        let weight = unsafe { NSFontWeightSemibold };
        let font = NSFont::monospacedDigitSystemFontOfSize_weight(size, weight);
        // SAFETY: the attribute names are AppKit constants.
        let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
        let values: [&AnyObject; 2] = [font.as_ref(), color.as_ref()];
        Self(NSDictionary::from_slices(&keys, &values))
    }

    pub fn size(&self, text: &str) -> NSSize {
        // SAFETY: the dictionary holds a font and a color under their attribute names.
        unsafe { NSString::from_str(text).sizeWithAttributes(Some(&self.0)) }
    }

    pub fn draw(&self, text: &str, at: NSPoint) {
        // SAFETY: as in `size`.
        unsafe { NSString::from_str(text).drawAtPoint_withAttributes(at, Some(&self.0)) }
    }

    /// Draws `text` centered in `rect`.
    pub fn draw_centered(&self, text: &str, rect: NSRect) {
        let size = self.size(text);
        self.draw(
            text,
            NSPoint::new(
                rect.origin.x + (rect.size.width - size.width) / 2.0,
                rect.origin.y + (rect.size.height - size.height) / 2.0,
            ),
        );
    }
}

/// A cursor from premultiplied RGBA pixels: `size_px` square, `size_points` wide on screen.
pub fn cursor(
    size_px: usize,
    pixels: &[u8],
    size_points: f64,
    hotspot: NSPoint,
) -> Retained<NSCursor> {
    // SAFETY: a null plane pointer makes the rep allocate its own buffer of
    // `size_px * 4 * size_px` bytes, which is filled right after.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            size_px as isize,
            size_px as isize,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            (size_px * 4) as isize,
            32,
        )
    }
    .expect("bitmap rep");
    // SAFETY: the rep's buffer is `size_px * size_px * 4` bytes, as created above.
    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), rep.bitmapData(), size_px * size_px * 4);
    }
    rep.setSize(NSSize::new(size_points, size_points));
    let image = NSImage::initWithSize(NSImage::alloc(), NSSize::new(size_points, size_points));
    image.addRepresentation(&rep);
    NSCursor::initWithImage_hotSpot(NSCursor::alloc(), &image, hotspot)
}

/// The Core Graphics display id of a screen.
pub fn display_id(screen: &NSScreen) -> Option<u32> {
    let key = NSString::from_str("NSScreenNumber");
    screen
        .deviceDescription()
        .objectForKey(&key)
        .and_then(|value| value.downcast::<NSNumber>().ok())
        .map(|n| n.unsignedIntValue())
}

/// Height of the main display in points, which AppKit's global coordinates flip around.
pub fn main_height(mtm: MainThreadMarker) -> f64 {
    NSScreen::screens(mtm)
        .firstObject()
        .map_or(0.0, |s| s.frame().size.height)
}

/// Drops `value` on a later turn of the main run loop, so windows can be released from
/// inside their own event handlers.
pub fn release_later<T: 'static>(value: T, mtm: MainThreadMarker) {
    let bound = MainThreadBound::new(value, mtm);
    DispatchQueue::main().exec_async(move || {
        let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
        drop(bound.into_inner(mtm));
    });
}
