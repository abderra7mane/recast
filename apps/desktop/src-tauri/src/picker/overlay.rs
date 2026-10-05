//! The picker's AppKit side: one borderless, transparent panel per display above the menu
//! bar and Dock. The panels never activate the app; the one under the pointer becomes key
//! so Space and Esc reach it.

use std::{cell::RefCell, rc::Rc};

use objc2::{
    AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained, runtime::AnyObject,
};
use objc2_app_kit::{
    NSBackingStoreType, NSBezierPath, NSColor, NSCursor, NSEvent, NSResponder, NSScreen,
    NSScreenSaverWindowLevel, NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindingRule,
    NSWindowCollectionBehavior, NSWindowSharingType, NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSPoint, NSRect, NSSize};
use recast_capture::picker::{
    Highlight, Outcome, PickMode, Picked, Picker, PickerDisplay, PickerWindow, Point,
    point_from_appkit, rect_from_appkit,
};
use recast_project::Rect;

use super::cursor;
use crate::appkit::{self, KeyPanel, TextStyle, color, fill, inset, ns_rect};

const KEY_ESCAPE: u16 = 53;
const KEY_SPACE: u16 = 49;

pub type Done = Box<dyn FnOnce(Option<Picked>)>;

struct Session {
    picker: Picker,
    main_height: f64,
    shown: Highlight,
    panels: Vec<Retained<KeyPanel>>,
    views: Vec<Retained<OverlayView>>,
    cursor: Retained<NSCursor>,
    done: Option<Done>,
}

type Shared = Rc<RefCell<Session>>;

pub struct ViewIvars {
    display: usize,
    session: Shared,
}

define_class!(
    // SAFETY: NSView has no subclassing requirements and the view doesn't implement Drop.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RecastPickerView"]
    #[ivars = ViewIvars]
    struct OverlayView;

    impl OverlayView {
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

        #[unsafe(method(cursorUpdate:))]
        fn cursor_update(&self, _event: &NSEvent) {
            self.ivars().session.borrow().cursor.set();
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, _event: &NSEvent) {
            self.ivars().session.borrow().cursor.set();
            self.update(|picker, at| picker.move_to(at));
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            self.update(|picker, at| picker.press(at));
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, _event: &NSEvent) {
            self.update(|picker, at| picker.drag_to(at));
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {
            let outcome = {
                let mut session = self.ivars().session.borrow_mut();
                let at = pointer(session.main_height);
                session.picker.release(at)
            };
            match outcome {
                Outcome::Picked(picked) => self.finish(Some(picked)),
                Outcome::Pending => redraw(&self.ivars().session),
            }
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, _event: &NSEvent) {
            self.finish(None);
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            match event.keyCode() {
                KEY_ESCAPE => self.finish(None),
                KEY_SPACE if !event.isARepeat() => {
                    self.ivars().session.borrow_mut().picker.toggle_display_mode();
                    redraw(&self.ivars().session);
                }
                _ => {}
            }
        }

        #[unsafe(method(cancelOperation:))]
        fn cancel_operation(&self, _sender: Option<&AnyObject>) {
            self.finish(None);
        }
    }
);

fn pointer(main_height: f64) -> Point {
    let at = NSEvent::mouseLocation();
    point_from_appkit(at.x, at.y, main_height)
}

/// Redraws the overlays when what they show has changed.
fn redraw(session: &Shared) {
    let mut s = session.borrow_mut();
    let highlight = s.picker.highlight();
    if highlight == s.shown {
        return;
    }
    s.shown = highlight;
    for view in &s.views {
        view.setNeedsDisplay(true);
    }
}

impl OverlayView {
    fn new(mtm: MainThreadMarker, display: usize, session: Shared, size: NSSize) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ViewIvars { display, session });
        let frame = NSRect::new(NSPoint::ZERO, size);
        // SAFETY: `initWithFrame:` is NSView's designated initializer.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    fn update(&self, change: impl FnOnce(&mut Picker, Point)) {
        let session = &self.ivars().session;
        {
            let mut s = session.borrow_mut();
            let at = pointer(s.main_height);
            change(&mut s.picker, at);
        }
        redraw(session);
    }

    fn finish(&self, picked: Option<Picked>) {
        let mtm = self.mtm();
        let (panels, views, done) = {
            let mut s = self.ivars().session.borrow_mut();
            (
                std::mem::take(&mut s.panels),
                std::mem::take(&mut s.views),
                s.done.take(),
            )
        };
        for panel in &panels {
            panel.orderOut(None);
        }
        NSCursor::arrowCursor().set();
        appkit::release_later((panels, views), mtm);
        if let Some(done) = done {
            done(picked);
        }
    }

    fn draw(&self) {
        let session = self.ivars().session.borrow();
        let display = &session.picker.displays()[self.ivars().display];
        let local = |r: &Rect| {
            ns_rect(&Rect {
                x: r.x - display.bounds.x,
                y: r.y - display.bounds.y,
                ..*r
            })
        };
        let bounds = self.bounds();
        let accent = |alpha: f64| color(0.04, 0.52, 1.0, alpha);
        match &session.shown {
            Highlight::Region {
                bounds: region,
                label,
            } => {
                let selection = local(region);
                let dim = NSBezierPath::bezierPathWithRect(bounds);
                dim.appendBezierPathWithRect(selection);
                dim.setWindingRule(NSWindingRule::EvenOdd);
                color(0.0, 0.0, 0.0, 0.35).setFill();
                dim.fill();
                color(1.0, 1.0, 1.0, 0.95).setStroke();
                let border = NSBezierPath::bezierPathWithRect(inset(selection, -0.5, -0.5));
                border.setLineWidth(1.0);
                border.stroke();
                if intersects(&selection, &bounds) {
                    region_label(label, selection, bounds);
                }
            }
            Highlight::Window {
                bounds: window,
                label,
            } => {
                // Nearly clear pixels still take the clicks that clear ones would pass on.
                fill(bounds, &color(0.0, 0.0, 0.0, 0.01));
                let r = local(window);
                fill(r, &accent(0.22));
                appkit::stroke_rounded(inset(r, 1.0, 1.0), 0.0, 2.0, &accent(0.9));
                let visible = intersection(&r, &bounds);
                if visible.size.width > 0.0 && visible.size.height > 0.0 {
                    pill_label(label, center(&visible));
                }
            }
            Highlight::Display {
                bounds: shown,
                label,
            } if *shown == display.bounds => {
                fill(bounds, &accent(0.18));
                appkit::stroke_rounded(inset(bounds, 2.0, 2.0), 0.0, 4.0, &accent(0.9));
                pill_label(label, center(&bounds));
            }
            _ => fill(bounds, &color(0.0, 0.0, 0.0, 0.01)),
        }
    }
}

fn intersects(a: &NSRect, b: &NSRect) -> bool {
    let r = intersection(a, b);
    r.size.width > 0.0 && r.size.height > 0.0
}

fn intersection(a: &NSRect, b: &NSRect) -> NSRect {
    let x0 = a.origin.x.max(b.origin.x);
    let y0 = a.origin.y.max(b.origin.y);
    let x1 = (a.origin.x + a.size.width).min(b.origin.x + b.size.width);
    let y1 = (a.origin.y + a.size.height).min(b.origin.y + b.size.height);
    NSRect::new(
        NSPoint::new(x0, y0),
        NSSize::new((x1 - x0).max(0.0), (y1 - y0).max(0.0)),
    )
}

fn center(r: &NSRect) -> NSPoint {
    NSPoint::new(
        r.origin.x + r.size.width / 2.0,
        r.origin.y + r.size.height / 2.0,
    )
}

const LABEL_PADDING: (f64, f64) = (10.0, 5.0);

fn label_size(style: &TextStyle, text: &str) -> NSSize {
    let size = style.size(text);
    NSSize::new(
        size.width + 2.0 * LABEL_PADDING.0,
        size.height + 2.0 * LABEL_PADDING.1,
    )
}

fn draw_pill(style: &TextStyle, text: &str, rect: NSRect) {
    appkit::fill_rounded(rect, 6.0, &color(0.0, 0.0, 0.0, 0.75));
    style.draw_centered(text, rect);
}

fn pill_label(text: &str, at: NSPoint) {
    let style = TextStyle::new(13.0, &NSColor::whiteColor());
    let size = label_size(&style, text);
    let rect = NSRect::new(
        NSPoint::new(at.x - size.width / 2.0, at.y - size.height / 2.0),
        size,
    );
    draw_pill(&style, text, rect);
}

/// The size label sits below the selection's bottom-right corner, or inside it when
/// there is no room below.
fn region_label(text: &str, selection: NSRect, bounds: NSRect) {
    let style = TextStyle::new(12.0, &NSColor::whiteColor());
    let size = label_size(&style, text);
    let right = selection.origin.x + selection.size.width;
    let bottom = selection.origin.y + selection.size.height;
    let x = (right - size.width).clamp(4.0, (bounds.size.width - size.width - 4.0).max(4.0));
    let y = if bottom + 8.0 + size.height <= bounds.size.height {
        bottom + 8.0
    } else {
        (bottom - size.height - 8.0).max(4.0)
    };
    draw_pill(&style, text, NSRect::new(NSPoint::new(x, y), size));
}

fn make_panel(mtm: MainThreadMarker, frame: NSRect) -> Retained<KeyPanel> {
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
    panel.setSharingType(NSWindowSharingType::None);
    panel.setHidesOnDeactivate(false);
    panel.setIgnoresMouseEvents(false);
    panel.setAcceptsMouseMovedEvents(true);
    panel.setFrame_display(frame, false);
    panel
}

/// Shows the overlays and calls `done` with the pick, or `None` when cancelled.
/// `windows` is ordered front to back.
pub fn open(
    mtm: MainThreadMarker,
    mode: PickMode,
    windows: Vec<PickerWindow>,
    own_pid: i32,
    done: Done,
) {
    let main_height = appkit::main_height(mtm);
    let mut displays = Vec::new();
    let mut frames = Vec::new();
    for screen in NSScreen::screens(mtm).iter() {
        let Some(id) = appkit::display_id(&screen) else {
            continue;
        };
        let frame = screen.frame();
        displays.push(PickerDisplay {
            id,
            name: screen.localizedName().to_string(),
            bounds: rect_from_appkit(&appkit::rect(frame), main_height),
            scale_factor: screen.backingScaleFactor(),
        });
        frames.push(frame);
    }
    if displays.is_empty() {
        done(None);
        return;
    }

    let (size_px, pixels) = cursor::camera_pixels(2.0);
    let camera = appkit::cursor(
        size_px,
        &pixels,
        cursor::SIZE_POINTS,
        NSPoint::new(cursor::HOTSPOT.0, cursor::HOTSPOT.1),
    );
    let mut picker = Picker::new(mode, displays, windows, own_pid);
    let at = pointer(main_height);
    picker.move_to(at);
    let session: Shared = Rc::new(RefCell::new(Session {
        shown: picker.highlight(),
        picker,
        main_height,
        panels: Vec::new(),
        views: Vec::new(),
        cursor: camera,
        done: Some(done),
    }));

    let mut key = 0;
    for (index, frame) in frames.iter().enumerate() {
        let panel = make_panel(mtm, *frame);
        let view = OverlayView::new(mtm, index, session.clone(), frame.size);
        let options = NSTrackingAreaOptions::MouseMoved
            | NSTrackingAreaOptions::CursorUpdate
            | NSTrackingAreaOptions::ActiveAlways
            | NSTrackingAreaOptions::InVisibleRect;
        // SAFETY: the owner is the view the area is added to, which outlives it.
        let area = unsafe {
            NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(),
                NSRect::ZERO,
                options,
                Some(&view),
                None,
            )
        };
        view.addTrackingArea(&area);
        panel.setContentView(Some(&view));
        if recast_capture::picker::contains(&session.borrow().picker.displays()[index].bounds, at) {
            key = index;
        }
        let mut s = session.borrow_mut();
        s.panels.push(panel);
        s.views.push(view);
    }

    let s = session.borrow();
    for panel in &s.panels {
        panel.orderFrontRegardless();
    }
    s.panels[key].makeKeyWindow();
    s.panels[key].makeFirstResponder(Some(&s.views[key]));
    s.cursor.set();
}
