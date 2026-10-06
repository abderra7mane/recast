//! The floating thumbnail shown after a screenshot, in the bottom-right corner of the
//! display it was taken on. It never activates the app.
//!
//! A click opens the screenshot for editing, a right-click shows its menu, a drag puts
//! the file into another app, and a drag or two-finger swipe to the right dismisses it.

use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use dispatch2::{DispatchQueue, DispatchTime, MainThreadBound};
use objc2::{
    AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained, runtime::ProtocolObject, sel,
};
use objc2_app_kit::{
    NSBackingStoreType, NSBezierPath, NSColor, NSCompositingOperation, NSDragOperation,
    NSDraggingContext, NSDraggingItem, NSDraggingSession, NSDraggingSource, NSEvent,
    NSEventModifierFlags, NSEventPhase, NSFloatingWindowLevel, NSImage, NSMenu, NSMenuItem,
    NSPanel, NSResponder, NSTrackingArea, NSTrackingAreaOptions, NSView,
    NSWindowCollectionBehavior, NSWindowSharingType, NSWindowStyleMask,
};
use objc2_foundation::{
    NSArray, NSData, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL,
};

use super::thumbnail_layout::{
    self as layout, DismissTimer, Gesture, MenuItem, ScrollPhase, SwipeUpdate, TrackpadSwipe,
};
use crate::appkit::{self, color, inset, ns_rect};

const CORNER_RADIUS: f64 = 10.0;
const FRAME_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Copy,
    Save,
    ShowInFinder,
    Edit,
    Delete,
    Close,
    /// The image was dropped into another app.
    DraggedOut,
}

impl From<MenuItem> for Action {
    fn from(item: MenuItem) -> Self {
        match item {
            MenuItem::Copy => Self::Copy,
            MenuItem::Save => Self::Save,
            MenuItem::ShowInFinder => Self::ShowInFinder,
            MenuItem::Edit => Self::Edit,
            MenuItem::Delete => Self::Delete,
            MenuItem::Close => Self::Close,
        }
    }
}

pub type OnAction = Box<dyn Fn(Action)>;

#[derive(Debug, Clone, Copy)]
struct Press {
    /// Pointer position in screen points (y up) at the press.
    at: NSPoint,
    /// The press in card coordinates (y down).
    local_y: f64,
    gesture: Gesture,
}

#[derive(Default)]
struct Pointer {
    hovered: bool,
    press: Option<Press>,
    dragging: bool,
    trackpad: TrackpadSwipe,
}

pub struct ViewIvars {
    image: Retained<NSImage>,
    image_size: (f64, f64),
    file: PathBuf,
    /// The card's resting origin in screen points.
    home: NSPoint,
    pointer: RefCell<Pointer>,
    timer: Arc<Mutex<DismissTimer>>,
    /// Bumped to stop a running animation.
    animation: Rc<Cell<u64>>,
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
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            let busy = {
                let mut pointer = self.ivars().pointer.borrow_mut();
                pointer.hovered = false;
                pointer.dragging || pointer.press.is_some()
            };
            if !busy {
                self.timer().resume(Instant::now());
            }
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if event.modifierFlags().contains(NSEventModifierFlags::Control) {
                self.show_menu(event);
                return;
            }
            self.stop_animation();
            let local_y = self.local(event).y;
            self.ivars().pointer.borrow_mut().press = Some(Press {
                at: NSEvent::mouseLocation(),
                local_y,
                gesture: Gesture::Undecided,
            });
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            let Some(press) = self.ivars().pointer.borrow().press else {
                return;
            };
            let now = NSEvent::mouseLocation();
            let (dx, dy) = (now.x - press.at.x, press.at.y - now.y);
            let gesture = match press.gesture {
                Gesture::Undecided => layout::classify(dx, dy),
                Gesture::Swipe if layout::swipe_strays(press.local_y + dy, self.card().1) => {
                    Gesture::DragOut
                }
                decided => decided,
            };
            match gesture {
                Gesture::Undecided => {}
                Gesture::Swipe => {
                    self.set_press_gesture(Gesture::Swipe);
                    self.move_card(layout::swipe_offset(dx));
                }
                Gesture::DragOut => {
                    self.move_card(0.0);
                    self.start_drag(event);
                }
            }
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {
            let press = self.ivars().pointer.borrow_mut().press.take();
            let Some(press) = press else {
                return;
            };
            match press.gesture {
                Gesture::Undecided => (self.ivars().on_action)(Action::Edit),
                Gesture::Swipe => {
                    let dx = NSEvent::mouseLocation().x - press.at.x;
                    if layout::swipe_dismisses(layout::swipe_offset(dx)) {
                        (self.ivars().on_action)(Action::Close);
                    } else {
                        self.snap_back();
                        self.resume_unless_hovered();
                    }
                }
                Gesture::DragOut => {}
            }
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            self.show_menu(event);
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            if !event.hasPreciseScrollingDeltas() {
                return;
            }
            let phase = if event.momentumPhase() != NSEventPhase::None {
                ScrollPhase::Other
            } else {
                match event.phase() {
                    NSEventPhase::Began => ScrollPhase::Began,
                    NSEventPhase::Changed => ScrollPhase::Changed,
                    NSEventPhase::Ended => ScrollPhase::Ended,
                    NSEventPhase::Cancelled => ScrollPhase::Cancelled,
                    _ => ScrollPhase::Other,
                }
            };
            let inverted = event.isDirectionInvertedFromDevice();
            let dx = layout::finger_delta(event.scrollingDeltaX(), inverted);
            let dy = layout::finger_delta(event.scrollingDeltaY(), inverted);
            let update = self
                .ivars()
                .pointer
                .borrow_mut()
                .trackpad
                .scroll(phase, dx, dy);
            match update {
                SwipeUpdate::Ignore => {}
                SwipeUpdate::Follow(offset) => {
                    self.stop_animation();
                    self.timer().pause(Instant::now());
                    self.move_card(offset);
                }
                SwipeUpdate::Dismiss => (self.ivars().on_action)(Action::Close),
                SwipeUpdate::SnapBack => {
                    self.snap_back();
                    self.resume_unless_hovered();
                }
            }
        }

        #[unsafe(method(thumbnailMenuItem:))]
        fn menu_item(&self, sender: &NSMenuItem) {
            if let Some(item) = MenuItem::from_tag(sender.tag()) {
                (self.ivars().on_action)(item.into());
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
            self.ivars().pointer.borrow_mut().dragging = false;
            if operation != NSDragOperation::None {
                (self.ivars().on_action)(Action::DraggedOut);
            } else {
                self.resume_unless_hovered();
            }
        }
    }
);

impl ThumbnailView {
    fn timer(&self) -> std::sync::MutexGuard<'_, DismissTimer> {
        self.ivars().timer.lock().expect("timer lock")
    }

    fn resume_unless_hovered(&self) {
        if !self.ivars().pointer.borrow().hovered {
            self.timer().resume(Instant::now());
        }
    }

    fn card(&self) -> (f64, f64) {
        let size = self.bounds().size;
        (size.width, size.height)
    }

    fn local(&self, event: &NSEvent) -> NSPoint {
        self.convertPoint_fromView(event.locationInWindow(), None)
    }

    fn set_press_gesture(&self, gesture: Gesture) {
        if let Some(press) = self.ivars().pointer.borrow_mut().press.as_mut() {
            press.gesture = gesture;
        }
    }

    /// Moves the card `offset` points right of its resting place.
    fn move_card(&self, offset: f64) {
        if let Some(window) = self.window() {
            let home = self.ivars().home;
            window.setFrameOrigin(NSPoint::new(home.x + offset, home.y));
        }
    }

    fn stop_animation(&self) {
        let generation = &self.ivars().animation;
        generation.set(generation.get() + 1);
    }

    fn snap_back(&self) {
        let Some(window) = self.window() else {
            return;
        };
        let from = window.frame().origin.x;
        let home = self.ivars().home.x;
        let panel = window
            .downcast::<NSPanel>()
            .expect("the thumbnail is a panel");
        self.stop_animation();
        animate(
            panel,
            self.ivars().animation.clone(),
            layout::SNAP_BACK,
            Box::new(move |t| (layout::snap_back_at(from, home, t), 1.0)),
            Box::new(|_| {}),
            self.mtm(),
        );
    }

    fn show_menu(&self, event: &NSEvent) {
        let mtm = self.mtm();
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        for entry in MenuItem::MENU {
            let Some(item) = entry else {
                menu.addItem(&NSMenuItem::separatorItem(mtm));
                continue;
            };
            // SAFETY: the action takes an NSMenuItem sender, as `menu_item` does.
            let menu_item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(item.title()),
                    Some(sel!(thumbnailMenuItem:)),
                    &NSString::new(),
                )
            };
            menu_item.setTag(item.tag());
            // SAFETY: the view outlives the menu, which only lives during the call below.
            unsafe { menu_item.setTarget(Some(self)) };
            menu.addItem(&menu_item);
        }
        self.timer().pause(Instant::now());
        NSMenu::popUpContextMenu_withEvent_forView(&menu, event, self);
        self.resume_unless_hovered();
    }

    fn start_drag(&self, event: &NSEvent) {
        {
            let mut pointer = self.ivars().pointer.borrow_mut();
            pointer.dragging = true;
            pointer.press = None;
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
        let ivars = self.ivars();
        NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
            bounds,
            CORNER_RADIUS,
            CORNER_RADIUS,
        )
        .addClip();
        appkit::fill(bounds, &color(0.12, 0.12, 0.13, 1.0));
        let image_rect = ns_rect(&layout::image_rect(self.card(), ivars.image_size));
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
        appkit::stroke_rounded(
            inset(bounds, 0.5, 0.5),
            CORNER_RADIUS,
            1.0,
            &color(1.0, 1.0, 1.0, 0.18),
        );
    }
}

/// Maps animation progress (0 to 1) to the card's x and opacity.
type Step = Box<dyn Fn(f64) -> (f64, f64)>;
type Finish = Box<dyn FnOnce(MainThreadMarker)>;

struct Animation {
    panel: Retained<NSPanel>,
    generation: Rc<Cell<u64>>,
    id: u64,
    start: Instant,
    duration: Duration,
    step: Step,
    finish: Option<Finish>,
}

/// Moves and fades `panel` frame by frame on the main queue; a newer animation on the
/// same `generation` stops it.
fn animate(
    panel: Retained<NSPanel>,
    generation: Rc<Cell<u64>>,
    duration: Duration,
    step: Step,
    finish: Finish,
    mtm: MainThreadMarker,
) {
    let id = generation.get();
    tick(
        Animation {
            panel,
            generation,
            id,
            start: Instant::now(),
            duration,
            step,
            finish: Some(finish),
        },
        mtm,
    );
}

fn tick(mut animation: Animation, mtm: MainThreadMarker) {
    if animation.generation.get() != animation.id {
        return;
    }
    let progress = (animation.start.elapsed().as_secs_f64()
        / animation.duration.as_secs_f64().max(1e-3))
    .min(1.0);
    let (x, alpha) = (animation.step)(progress);
    let y = animation.panel.frame().origin.y;
    animation.panel.setFrameOrigin(NSPoint::new(x, y));
    animation.panel.setAlphaValue(alpha);
    if progress >= 1.0 {
        if let Some(finish) = animation.finish.take() {
            finish(mtm);
        }
        return;
    }
    let next = MainThreadBound::new(animation, mtm);
    let when = DispatchTime::try_from(FRAME_INTERVAL).unwrap_or(DispatchTime::NOW);
    let queued = DispatchQueue::main().after(when, move || {
        let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
        tick(next.into_inner(mtm), mtm);
    });
    if queued.is_err() {
        log::warn!("cannot animate the thumbnail");
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
    let Some(screen) = appkit::screen_for(thumbnail.display_id, mtm) else {
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
        home: frame.origin,
        pointer: RefCell::default(),
        timer: timer.clone(),
        animation: Rc::default(),
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

/// Slides the thumbnail with `id`, or whichever is shown when `id` is `None`, out to the
/// right and releases it.
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
    let Some(Shown {
        panel,
        view,
        closed,
        ..
    }) = taken
    else {
        return;
    };
    closed.store(true, Ordering::SeqCst);
    panel.setIgnoresMouseEvents(true);
    let frame = appkit::rect(panel.frame());
    let Some(screen) = panel.screen() else {
        panel.orderOut(None);
        appkit::release_later((panel, view), mtm);
        return;
    };
    let end_x = layout::slide_out_frame(&frame, &appkit::rect(screen.frame())).x;
    let generation = view.ivars().animation.clone();
    generation.set(generation.get() + 1);
    let width = frame.width;
    let start_x = frame.x;
    let finish_panel = panel.clone();
    animate(
        panel,
        generation,
        layout::SLIDE_OUT,
        Box::new(move |t| layout::slide_out_at(start_x, end_x, width, t)),
        Box::new(move |mtm| {
            finish_panel.orderOut(None);
            appkit::release_later((finish_panel, view), mtm);
        }),
        mtm,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_items_become_their_actions() {
        let actions: Vec<Action> = MenuItem::MENU
            .into_iter()
            .flatten()
            .map(Action::from)
            .collect();
        assert_eq!(
            actions,
            [
                Action::Copy,
                Action::Save,
                Action::ShowInFinder,
                Action::Edit,
                Action::Delete,
                Action::Close
            ]
        );
    }
}
