//! Small AppKit helpers shared by the picker overlay and the screenshot thumbnail.

use dispatch2::{DispatchQueue, MainThreadBound};
use objc2::{
    AllocAnyThread, MainThreadMarker, MainThreadOnly, define_class, rc::Retained,
    runtime::AnyObject,
};
use objc2_app_kit::{
    NSBezierPath, NSBitmapImageRep, NSColor, NSCursor, NSDeviceRGBColorSpace, NSFont,
    NSFontAttributeName, NSFontWeightSemibold, NSForegroundColorAttributeName, NSImage, NSPanel,
    NSResponder, NSScreen, NSStringDrawing, NSWindow,
};
use objc2_foundation::{NSDictionary, NSNumber, NSObject, NSPoint, NSRect, NSSize, NSString};
use recast_project::Rect;

define_class!(
    // SAFETY: NSPanel has no subclassing requirements; only a getter is overridden.
    #[unsafe(super(NSPanel, NSWindow, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RecastKeyPanel"]
    /// A panel that can become key without a title bar, so it receives Esc.
    pub struct KeyPanel;

    impl KeyPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            true
        }
    }
);

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
        Self::with_weight(size, unsafe { NSFontWeightSemibold }, color)
    }

    pub fn with_weight(size: f64, weight: f64, color: &NSColor) -> Self {
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

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn _CGSDefaultConnection() -> i32;
    fn CGSSetConnectionProperty(
        connection: i32,
        target: i32,
        key: *const std::ffi::c_void,
        value: *const std::ffi::c_void,
    ) -> i32;
}

/// Lets Recast set the cursor while another app is active. macOS doesn't always grant
/// the activation an overlay asks for, and it ignores a background app's cursor
/// otherwise. This is the private window server property screenshot tools rely on.
pub fn set_cursor_in_background() {
    let key = NSString::from_str("SetsCursorInBackground");
    let value = NSNumber::new_bool(true);
    // SAFETY: NSString and NSNumber are toll-free bridged with CFString and CFBoolean,
    // and both outlive the call.
    let status = unsafe {
        let connection = _CGSDefaultConnection();
        CGSSetConnectionProperty(
            connection,
            connection,
            (&*key as *const NSString).cast(),
            (&*value as *const NSNumber).cast(),
        )
    };
    if status != 0 {
        log::warn!("cannot set the cursor in the background: CGError {status}");
    }
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

/// The screen showing display `id`, or the main screen when it is gone.
pub fn screen_for(id: u32, mtm: MainThreadMarker) -> Option<Retained<NSScreen>> {
    NSScreen::screens(mtm)
        .iter()
        .find(|s| display_id(s) == Some(id))
        .or_else(|| NSScreen::mainScreen(mtm))
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

/// Hides Recast's own overlays from screen capture. QA builds with the `synthetic`
/// feature can set `RECAST_QA_CAPTURABLE=1` to let screenshots see them.
pub fn hide_from_capture(window: &NSWindow) {
    window.setSharingType(overlay_sharing(qa_capturable()));
}

#[cfg(feature = "synthetic")]
fn qa_capturable() -> bool {
    std::env::var_os("RECAST_QA_CAPTURABLE").is_some_and(|v| v == "1")
}

#[cfg(not(feature = "synthetic"))]
fn qa_capturable() -> bool {
    false
}

fn overlay_sharing(capturable: bool) -> objc2_app_kit::NSWindowSharingType {
    if capturable {
        objc2_app_kit::NSWindowSharingType::ReadOnly
    } else {
        objc2_app_kit::NSWindowSharingType::None
    }
}

#[cfg(test)]
mod sharing_tests {
    use super::*;

    #[test]
    fn overlays_are_hidden_unless_qa_capturable() {
        assert_eq!(
            overlay_sharing(false),
            objc2_app_kit::NSWindowSharingType::None
        );
        assert_eq!(
            overlay_sharing(true),
            objc2_app_kit::NSWindowSharingType::ReadOnly
        );
    }

    #[cfg(not(feature = "synthetic"))]
    #[test]
    fn release_builds_ignore_the_qa_switch() {
        assert!(!qa_capturable());
    }
}
