//! The floating thumbnail shown after a screenshot, in the bottom-right corner of the
//! display it was taken on. It never activates the app.

use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use dispatch2::DispatchQueue;
use objc2::{
    AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained, runtime::ProtocolObject,
};
use objc2_app_kit::{
    NSBackingStoreType, NSBezierPath, NSColor, NSCompositingOperation, NSDragOperation,
    NSDraggingContext, NSDraggingItem, NSDraggingSession, NSDraggingSource, NSEvent,
    NSFloatingWindowLevel, NSImage, NSPanel, NSResponder, NSScreen, NSTrackingArea,
    NSTrackingAreaOptions, NSView, NSWindowCollectionBehavior, NSWindowSharingType,
    NSWindowStyleMask,
};
use objc2_foundation::{
    NSArray, NSData, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL,
};

use super::thumbnail_layout::{self as layout, Button, DismissTimer};
use crate::appkit::{self, TextStyle, color, inset, ns_rect};

const DRAG_THRESHOLD: f64 = 3.0;
const CORNER_RADIUS: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Copy,
    Save,
    Beautify,
    Close,
    /// The image was dropped into another app.
    DraggedOut,
}

pub type OnAction = Box<dyn Fn(Action)>;

#[derive(Default)]
struct Pointer {
    hovered: bool,
    pressed: Option<NSPoint>,
    dragging: bool,
}

pub struct ViewIvars {
    image: Retained<NSImage>,
    image_size: (f64, f64),
    file: PathBuf,
    pointer: RefCell<Pointer>,
    timer: Arc<Mutex<DismissTimer>>,
    on_action: OnAction,
}

define_class!(
    // SAFETY: NSView has no subclassing requirements and the view doesn't implement Drop.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RecastThumbnailView"]
    #[ivars = ViewIvars]
    struct ThumbnailView;

    impl ThumbnailView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            self.draw();
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            self.ivars().pointer.borrow_mut().hovered = true;
            self.timer().pause(Instant::now());
            self.setNeedsDisplay(true);
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            let dragging = {
                let mut pointer = self.ivars().pointer.borrow_mut();
                pointer.hovered = false;
                pointer.dragging
            };
            if !dragging {
                self.timer().resume(Instant::now());
            }
            self.setNeedsDisplay(true);
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            self.ivars().pointer.borrow_mut().pressed = Some(self.local(event));
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            let at = self.local(event);
            let start = {
                let pointer = self.ivars().pointer.borrow();
                match pointer.pressed {
                    Some(p) if !pointer.dragging && self.button_at(p).is_none() => p,
                    _ => return,
                }
            };
            if (at.x - start.x).hypot(at.y - start.y) >= DRAG_THRESHOLD {
                self.start_drag(event);
            }
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            let pressed = self.ivars().pointer.borrow_mut().pressed.take();
            let Some(pressed) = pressed else {
                return;
            };
            let button = self.button_at(pressed);
            if button.is_some() && button == self.button_at(self.local(event)) {
                (self.ivars().on_action)(match button.expect("checked") {
                    Button::Close => Action::Close,
                    Button::Copy => Action::Copy,
                    Button::Save => Action::Save,
                    Button::Beautify => Action::Beautify,
                });
            }
        }
    }

    unsafe impl NSObjectProtocol for ThumbnailView {}

    unsafe impl NSDraggingSource for ThumbnailView {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn source_operation_mask(
            &self,
            _session: &NSDraggingSession,
            _context: NSDraggingContext,
        ) -> NSDragOperation {
            NSDragOperation::Copy
        }

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn dragging_ended(
            &self,
            _session: &NSDraggingSession,
            _point: NSPoint,
            operation: NSDragOperation,
        ) {
            let hovered = {
                let mut pointer = self.ivars().pointer.borrow_mut();
                pointer.dragging = false;
                pointer.hovered
            };
            if operation != NSDragOperation::None {
                (self.ivars().on_action)(Action::DraggedOut);
            } else if !hovered {
                self.timer().resume(Instant::now());
            }
        }
    }
);

impl ThumbnailView {
    fn timer(&self) -> std::sync::MutexGuard<'_, DismissTimer> {
        self.ivars().timer.lock().expect("timer lock")
    }

    fn card(&self) -> (f64, f64) {
        let size = self.bounds().size;
        (size.width, size.height)
    }

    fn local(&self, event: &NSEvent) -> NSPoint {
        self.convertPoint_fromView(event.locationInWindow(), None)
    }

    fn button_at(&self, p: NSPoint) -> Option<Button> {
        layout::button_at(self.card(), p.x, p.y)
    }

    fn start_drag(&self, event: &NSEvent) {
        {
            let mut pointer = self.ivars().pointer.borrow_mut();
            pointer.dragging = true;
            pointer.pressed = None;
        }
        self.timer().pause(Instant::now());
        let path = NSString::from_str(&self.ivars().file.to_string_lossy());
        let url = NSURL::fileURLWithPath(&path);
        let item = NSDraggingItem::initWithPasteboardWriter(
            NSDraggingItem::alloc(),
            ProtocolObject::from_ref(&*url),
        );
        // SAFETY: an NSImage is a valid dragging image.
        unsafe { item.setDraggingFrame_contents(self.bounds(), Some(&self.ivars().image)) };
        self.beginDraggingSessionWithItems_event_source(
            &NSArray::from_retained_slice(&[item]),
            event,
            ProtocolObject::from_ref(self),
        );
    }

    fn draw(&self) {
        let bounds = self.bounds();
        let card = self.card();
        let ivars = self.ivars();
        NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
            bounds,
            CORNER_RADIUS,
            CORNER_RADIUS,
        )
        .addClip();
        appkit::fill(bounds, &color(0.12, 0.12, 0.13, 1.0));
        let image_rect = ns_rect(&layout::image_rect(card, ivars.image_size));
        // SAFETY: a zero source rect draws the whole image; no hints are passed.
        unsafe {
            ivars
                .image
                .drawInRect_fromRect_operation_fraction_respectFlipped_hints(
                    image_rect,
                    NSRect::ZERO,
                    NSCompositingOperation::SourceOver,
                    1.0,
                    true,
                    None,
                )
        };
        if ivars.pointer.borrow().hovered {
            appkit::fill(bounds, &color(0.0, 0.0, 0.0, 0.45));
            let dark = TextStyle::new(13.0, &color(0.1, 0.1, 0.1, 1.0));
            let light = TextStyle::new(11.0, &NSColor::whiteColor());
            for (button, rect) in layout::buttons(card) {
                let r = ns_rect(&rect);
                if button == Button::Close {
                    appkit::fill_rounded(r, r.size.width / 2.0, &color(0.0, 0.0, 0.0, 0.65));
                    light.draw_centered(button.label(), r);
                } else {
                    appkit::fill_rounded(r, r.size.height / 2.0, &color(1.0, 1.0, 1.0, 0.92));
                    dark.draw_centered(button.label(), r);
                }
            }
        }
        appkit::stroke_rounded(
            inset(bounds, 0.5, 0.5),
            CORNER_RADIUS,
            1.0,
            &color(1.0, 1.0, 1.0, 0.18),
        );
    }
}

struct Shown {
    id: u64,
    panel: Retained<NSPanel>,
    view: Retained<ThumbnailView>,
    closed: Arc<AtomicBool>,
}

thread_local! {
    static SHOWN: RefCell<Option<Shown>> = const { RefCell::new(None) };
}

fn screen_for(display_id: u32, mtm: MainThreadMarker) -> Option<Retained<NSScreen>> {
    let screens = NSScreen::screens(mtm);
    screens
        .iter()
        .find(|s| appkit::display_id(s) == Some(display_id))
        .or_else(|| NSScreen::mainScreen(mtm))
}

pub struct Thumbnail {
    pub id: u64,
    pub png: Vec<u8>,
    /// Image size in points.
    pub size: (f64, f64),
    /// The PNG on disk, for dragging into other apps.
    pub file: PathBuf,
    pub display_id: u32,
}

/// Shows the thumbnail in place of any other one; it closes by itself after a while
/// unless the pointer is over it.
pub fn show(mtm: MainThreadMarker, thumbnail: Thumbnail, on_action: OnAction) {
    close(None, mtm);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(&thumbnail.png))
    else {
        log::warn!("cannot show the thumbnail: unreadable image");
        return;
    };
    let Some(screen) = screen_for(thumbnail.display_id, mtm) else {
        return;
    };
    let card = layout::card_size(thumbnail.size.0, thumbnail.size.1);
    let (x, y) = layout::card_origin(&appkit::rect(screen.visibleFrame()), card);
    let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(card.0, card.1));

    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        frame,
        NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
        NSBackingStoreType::Buffered,
        false,
    );
    // SAFETY: the panel is owned through `Retained`, so AppKit must not release it on close.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(true);
    panel.setLevel(NSFloatingWindowLevel);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    panel.setSharingType(NSWindowSharingType::None);
    panel.setHidesOnDeactivate(false);

    let timer = Arc::new(Mutex::new(DismissTimer::start(
        layout::DISMISS_AFTER,
        Instant::now(),
    )));
    let view = ThumbnailView::alloc(mtm).set_ivars(ViewIvars {
        image,
        image_size: thumbnail.size,
        file: thumbnail.file,
        pointer: RefCell::default(),
        timer: timer.clone(),
        on_action,
    });
    let content = NSRect::new(NSPoint::ZERO, frame.size);
    // SAFETY: `initWithFrame:` is NSView's designated initializer.
    let view: Retained<ThumbnailView> = unsafe { msg_send![super(view), initWithFrame: content] };
    // SAFETY: the owner is the view the area is added to, which outlives it.
    let area = unsafe {
        NSTrackingArea::initWithRect_options_owner_userInfo(
            NSTrackingArea::alloc(),
            NSRect::ZERO,
            NSTrackingAreaOptions::MouseEnteredAndExited
                | NSTrackingAreaOptions::ActiveAlways
                | NSTrackingAreaOptions::InVisibleRect,
            Some(&view),
            None,
        )
    };
    view.addTrackingArea(&area);
    panel.setContentView(Some(&view));
    // A pointer already over the card gets no `mouseEntered:`, so start out hovered.
    let pointer = NSEvent::mouseLocation();
    let inside = pointer.x >= frame.origin.x
        && pointer.x < frame.origin.x + frame.size.width
        && pointer.y >= frame.origin.y
        && pointer.y < frame.origin.y + frame.size.height;
    if inside {
        view.ivars().pointer.borrow_mut().hovered = true;
        timer.lock().expect("timer lock").pause(Instant::now());
    }
    panel.orderFrontRegardless();
    panel.invalidateShadow();

    let closed = Arc::new(AtomicBool::new(false));
    let id = thumbnail.id;
    SHOWN.with_borrow_mut(|shown| {
        *shown = Some(Shown {
            id,
            panel,
            view,
            closed: closed.clone(),
        })
    });
    let spawned = std::thread::Builder::new()
        .name("thumbnail-timer".into())
        .spawn(move || {
            while !closed.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(100));
                if timer.lock().expect("timer lock").expired(Instant::now()) {
                    DispatchQueue::main().exec_async(move || {
                        let mtm = MainThreadMarker::new().expect("main queue");
                        close(Some(id), mtm);
                    });
                    return;
                }
            }
        });
    if let Err(e) = spawned {
        log::warn!("the thumbnail won't close by itself: {e}");
    }
}

/// Closes the thumbnail with `id`, or whichever is shown when `id` is `None`.
pub fn close(id: Option<u64>, mtm: MainThreadMarker) {
    let taken = SHOWN.with_borrow_mut(|shown| {
        if shown
            .as_ref()
            .is_some_and(|s| id.is_none_or(|id| s.id == id))
        {
            shown.take()
        } else {
            None
        }
    });
    if let Some(shown) = taken {
        shown.closed.store(true, Ordering::SeqCst);
        shown.panel.orderOut(None);
        appkit::release_later((shown.panel, shown.view), mtm);
    }
}
