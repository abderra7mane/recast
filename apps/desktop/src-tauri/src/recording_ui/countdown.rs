//! The countdown before recording: a large number over the target display with the
//! recorded area outlined. A click, Return or Space starts recording right away; Esc
//! cancels it. It is left out of captures.

use std::{
    cell::{Cell, RefCell},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use dispatch2::DispatchQueue;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_app_kit::{
    NSBackingStoreType, NSBezierPath, NSColor, NSEvent, NSResponder, NSScreenSaverWindowLevel,
    NSView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSPoint, NSRect, NSSize};
use recast_capture::picker::rect_from_appkit;
use recast_project::Rect;

use crate::appkit::{self, KeyPanel, TextStyle, color, inset, ns_rect};

const KEY_ESCAPE: u16 = 53;
const KEY_RETURN: u16 = 36;
const KEY_SPACE: u16 = 49;
const BOX_SIZE: f64 = 168.0;
const HINT_LINE: f64 = 15.0;

/// How the countdown ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// It ran out or was skipped: start recording.
    Start,
    /// The user pressed Esc: don't record.
    Cancel,
}

pub type Done = Box<dyn FnOnce(Ended)>;

pub struct ViewIvars {
    remaining: Cell<u32>,
    /// The recorded area in view coordinates.
    target: Option<Rect>,
}

define_class!(
    // SAFETY: NSView has no subclassing requirements and the view doesn't implement Drop.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RecastCountdownView"]
    #[ivars = ViewIvars]
    struct CountdownView;

    impl CountdownView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
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

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            finish(self.mtm(), Ended::Start);
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            match event.keyCode() {
                KEY_RETURN | KEY_SPACE => finish(self.mtm(), Ended::Start),
                KEY_ESCAPE => finish(self.mtm(), Ended::Cancel),
                _ => {}
            }
        }

        #[unsafe(method(cancelOperation:))]
        fn cancel_operation(&self, _sender: Option<&objc2::runtime::AnyObject>) {
            finish(self.mtm(), Ended::Cancel);
        }
    }
);

impl CountdownView {
    fn draw(&self) {
        let bounds = self.bounds();
        let ivars = self.ivars();
        let dim = NSBezierPath::bezierPathWithRect(bounds);
        if let Some(target) = &ivars.target {
            let hole = ns_rect(target);
            dim.appendBezierPathWithRect(hole);
            dim.setWindingRule(objc2_app_kit::NSWindingRule::EvenOdd);
            color(0.0, 0.0, 0.0, 0.35).setFill();
            dim.fill();
            appkit::stroke_rounded(
                inset(hole, -1.0, -1.0),
                0.0,
                2.0,
                &color(1.0, 0.27, 0.23, 0.9),
            );
        } else {
            color(0.0, 0.0, 0.0, 0.2).setFill();
            dim.fill();
        }

        let card = NSRect::new(
            NSPoint::new(
                (bounds.size.width - BOX_SIZE) / 2.0,
                (bounds.size.height - BOX_SIZE) / 2.0,
            ),
            NSSize::new(BOX_SIZE, BOX_SIZE),
        );
        appkit::fill_rounded(card, 28.0, &color(0.08, 0.08, 0.09, 0.82));
        let number = TextStyle::new(96.0, &NSColor::whiteColor());
        let digit_area = NSRect::new(card.origin, NSSize::new(BOX_SIZE, BOX_SIZE - 34.0));
        number.draw_centered(&ivars.remaining.get().to_string(), digit_area);
        let hint = TextStyle::with_weight(11.0, 0.0, &color(1.0, 1.0, 1.0, 0.7));
        for (line, text) in ["Click to start now", "Esc to cancel"]
            .into_iter()
            .enumerate()
        {
            let line_area = NSRect::new(
                NSPoint::new(
                    card.origin.x,
                    card.origin.y + BOX_SIZE - 50.0 + line as f64 * HINT_LINE,
                ),
                NSSize::new(BOX_SIZE, HINT_LINE),
            );
            hint.draw_centered(text, line_area);
        }
    }
}

struct Shown {
    panel: Retained<KeyPanel>,
    view: Retained<CountdownView>,
    stopped: Arc<AtomicBool>,
    done: Option<Done>,
}

thread_local! {
    static SHOWN: RefCell<Option<Shown>> = const { RefCell::new(None) };
}

/// Counts down from `seconds` over display `display_id`, then calls `done` with how it
/// ended. `target` is
/// the recorded area in global points (y down), outlined when given.
pub fn show(
    mtm: MainThreadMarker,
    display_id: u32,
    target: Option<Rect>,
    seconds: u32,
    done: Done,
) {
    close(mtm);
    let Some(screen) = appkit::screen_for(display_id, mtm) else {
        done(Ended::Start);
        return;
    };
    let frame = screen.frame();
    let display = rect_from_appkit(&appkit::rect(frame), appkit::main_height(mtm));
    let target = target.map(|t| Rect {
        x: t.x - display.x,
        y: t.y - display.y,
        ..t
    });

    let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
    // SAFETY: NSPanel's designated initializer with valid arguments.
    let panel: Retained<KeyPanel> = unsafe {
        msg_send![
            KeyPanel::alloc(mtm),
            initWithContentRect: frame,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false
        ]
    };
    // SAFETY: the panel is owned through `Retained`, so AppKit must not release it on close.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(false);
    panel.setLevel(NSScreenSaverWindowLevel);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    crate::appkit::hide_from_capture(&panel);
    panel.setHidesOnDeactivate(false);

    let view = CountdownView::alloc(mtm).set_ivars(ViewIvars {
        remaining: Cell::new(seconds.max(1)),
        target,
    });
    let content = NSRect::new(NSPoint::ZERO, frame.size);
    // SAFETY: `initWithFrame:` is NSView's designated initializer.
    let view: Retained<CountdownView> = unsafe { msg_send![super(view), initWithFrame: content] };
    panel.setContentView(Some(&view));
    panel.orderFrontRegardless();
    panel.makeKeyWindow();
    panel.makeFirstResponder(Some(&view));

    let stopped = Arc::new(AtomicBool::new(false));
    SHOWN.with_borrow_mut(|shown| {
        *shown = Some(Shown {
            panel,
            view,
            stopped: stopped.clone(),
            done: Some(done),
        })
    });
    let spawned = std::thread::Builder::new()
        .name("countdown".into())
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(1));
                if stopped.load(Ordering::SeqCst) {
                    return;
                }
                DispatchQueue::main().exec_async(tick);
            }
        });
    if let Err(e) = spawned {
        log::warn!("the countdown can't run, starting right away: {e}");
        finish(mtm, Ended::Start);
    }
}

fn tick() {
    let mtm = MainThreadMarker::new().expect("main queue");
    let ended = SHOWN.with_borrow(|shown| {
        let Some(shown) = shown else {
            return false;
        };
        let remaining = shown.view.ivars().remaining.get().saturating_sub(1);
        shown.view.ivars().remaining.set(remaining);
        shown.view.setNeedsDisplay(true);
        remaining == 0
    });
    if ended {
        finish(mtm, Ended::Start);
    }
}

fn take(mtm: MainThreadMarker) -> Option<Done> {
    let shown = SHOWN.with_borrow_mut(Option::take)?;
    shown.stopped.store(true, Ordering::SeqCst);
    shown.panel.orderOut(None);
    let done = shown.done;
    appkit::release_later((shown.panel, shown.view), mtm);
    done
}

/// Ends the countdown now and calls its `done`.
fn finish(mtm: MainThreadMarker, ended: Ended) {
    if let Some(done) = take(mtm) {
        done(ended);
    }
}

/// Removes the countdown without calling its `done`.
pub fn close(mtm: MainThreadMarker) {
    take(mtm);
}
