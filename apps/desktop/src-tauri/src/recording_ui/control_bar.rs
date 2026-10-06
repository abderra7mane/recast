//! The floating bar shown while recording: elapsed time, Stop, Restart and Cancel. It
//! stays above other windows, can be dragged, never activates Recast and is left out of
//! captures.

use std::{
    cell::{Cell, RefCell},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use dispatch2::DispatchQueue;
use objc2::{
    AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSEvent, NSPanel, NSResponder, NSStatusWindowLevel,
    NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSPoint, NSRect, NSSize};

use super::bar_layout::{self as layout, BarButton};
use crate::appkit::{self, TextStyle, color, inset, ns_rect};

pub type OnButton = Box<dyn Fn(BarButton)>;

pub struct BarIvars {
    started: Instant,
    hovered: Cell<Option<BarButton>>,
    pressed: Cell<Option<BarButton>>,
    on_button: OnButton,
}

define_class!(
    // SAFETY: NSView has no subclassing requirements and the view doesn't implement Drop.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RecastControlBarView"]
    #[ivars = BarIvars]
    struct BarView;

    impl BarView {
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

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            self.hover(self.button_at(event));
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            self.hover(None);
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            match self.button_at(event) {
                Some(button) => {
                    self.ivars().pressed.set(Some(button));
                    self.setNeedsDisplay(true);
                }
                None => {
                    if let Some(window) = self.window() {
                        window.performWindowDragWithEvent(event);
                    }
                }
            }
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            let pressed = self.ivars().pressed.take();
            self.setNeedsDisplay(true);
            if let Some(button) = pressed
                && self.button_at(event) == Some(button)
            {
                (self.ivars().on_button)(button);
            }
        }
    }
);

impl BarView {
    fn button_at(&self, event: &NSEvent) -> Option<BarButton> {
        let p = self.convertPoint_fromView(event.locationInWindow(), None);
        layout::button_at(p.x, p.y)
    }

    fn hover(&self, button: Option<BarButton>) {
        if self.ivars().hovered.replace(button) != button {
            self.setNeedsDisplay(true);
        }
    }

    fn draw(&self) {
        let bounds = self.bounds();
        let ivars = self.ivars();
        let radius = bounds.size.height / 2.0;
        appkit::fill_rounded(bounds, radius, &color(0.11, 0.11, 0.12, 0.94));
        appkit::stroke_rounded(
            inset(bounds, 0.5, 0.5),
            radius,
            1.0,
            &color(1.0, 1.0, 1.0, 0.16),
        );

        let dot = NSRect::new(
            NSPoint::new(16.0, bounds.size.height / 2.0 - 6.0),
            NSSize::new(12.0, 12.0),
        );
        appkit::fill_rounded(dot, 6.0, &color(1.0, 0.27, 0.23, 1.0));

        let time = layout::elapsed_text(ivars.started.elapsed().as_secs_f64() * 1000.0);
        let time_style = TextStyle::new(15.0, &NSColor::whiteColor());
        let area = ns_rect(&layout::time_rect());
        let size = time_style.size(&time);
        time_style.draw(
            &time,
            NSPoint::new(
                area.origin.x,
                area.origin.y + (area.size.height - size.height) / 2.0,
            ),
        );

        let label = TextStyle::new(13.0, &NSColor::whiteColor());
        for (button, rect) in layout::buttons() {
            let r = ns_rect(&rect);
            let lit = ivars.hovered.get() == Some(button);
            let down = ivars.pressed.get() == Some(button);
            let fill = match button {
                BarButton::Stop => color(0.86, 0.2, 0.18, if down { 0.8 } else { 1.0 }),
                _ => color(
                    1.0,
                    1.0,
                    1.0,
                    match (down, lit) {
                        (true, _) => 0.3,
                        (false, true) => 0.22,
                        (false, false) => 0.12,
                    },
                ),
            };
            appkit::fill_rounded(r, r.size.height / 2.0, &fill);
            if lit && button == BarButton::Stop {
                appkit::fill_rounded(r, r.size.height / 2.0, &color(1.0, 1.0, 1.0, 0.12));
            }
            label.draw_centered(button.label(), r);
        }
    }
}

struct Shown {
    panel: Retained<NSPanel>,
    view: Retained<BarView>,
    closed: Arc<AtomicBool>,
}

thread_local! {
    static SHOWN: RefCell<Option<Shown>> = const { RefCell::new(None) };
}

/// Shows the bar near the bottom of display `display_id`; `started` is when recording began.
pub fn show(mtm: MainThreadMarker, display_id: u32, started: Instant, on_button: OnButton) {
    close(mtm);
    let Some(screen) = appkit::screen_for(display_id, mtm) else {
        return;
    };
    let (x, y) = layout::origin(&appkit::rect(screen.visibleFrame()));
    let frame = NSRect::new(
        NSPoint::new(x, y),
        NSSize::new(layout::WIDTH, layout::HEIGHT),
    );
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
    panel.setLevel(NSStatusWindowLevel);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    crate::appkit::hide_from_capture(&panel);
    panel.setHidesOnDeactivate(false);
    panel.setAcceptsMouseMovedEvents(true);

    let view = BarView::alloc(mtm).set_ivars(BarIvars {
        started,
        hovered: Cell::new(None),
        pressed: Cell::new(None),
        on_button,
    });
    let content = NSRect::new(NSPoint::ZERO, frame.size);
    // SAFETY: `initWithFrame:` is NSView's designated initializer.
    let view: Retained<BarView> = unsafe { msg_send![super(view), initWithFrame: content] };
    // SAFETY: the owner is the view the area is added to, which outlives it.
    let area = unsafe {
        NSTrackingArea::initWithRect_options_owner_userInfo(
            NSTrackingArea::alloc(),
            NSRect::ZERO,
            NSTrackingAreaOptions::MouseMoved
                | NSTrackingAreaOptions::MouseEnteredAndExited
                | NSTrackingAreaOptions::ActiveAlways
                | NSTrackingAreaOptions::InVisibleRect,
            Some(&view),
            None,
        )
    };
    view.addTrackingArea(&area);
    panel.setContentView(Some(&view));
    panel.orderFrontRegardless();
    panel.invalidateShadow();

    let closed = Arc::new(AtomicBool::new(false));
    SHOWN.with_borrow_mut(|shown| {
        *shown = Some(Shown {
            panel,
            view,
            closed: closed.clone(),
        })
    });
    let spawned = std::thread::Builder::new()
        .name("control-bar-clock".into())
        .spawn(move || {
            while !closed.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(250));
                DispatchQueue::main().exec_async(|| {
                    SHOWN.with_borrow(|shown| {
                        if let Some(shown) = shown {
                            shown.view.setNeedsDisplay(true);
                        }
                    });
                });
            }
        });
    if let Err(e) = spawned {
        log::warn!("the control bar clock won't run: {e}");
    }
}

pub fn close(mtm: MainThreadMarker) {
    if let Some(shown) = SHOWN.with_borrow_mut(Option::take) {
        shown.closed.store(true, Ordering::SeqCst);
        shown.panel.orderOut(None);
        appkit::release_later((shown.panel, shown.view), mtm);
    }
}
