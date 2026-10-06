//! The picker's AppKit side: one borderless, transparent panel per display above the menu
//! bar and Dock, with a hint at the top of the display under the pointer. The panels never
//! activate the app. The one under the pointer is key, so Esc reaches it. The system
//! cursor is hidden while they are up and the views draw the picker's cursor instead:
//! macOS shows the cursor of an app that isn't active only unreliably.

use std::{cell::RefCell, rc::Rc};

use objc2::{
    AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained, runtime::AnyObject,
};
use objc2_app_kit::{
    NSBackingStoreType, NSBezierPath, NSColor, NSCursor, NSEvent, NSImage, NSResponder, NSScreen,
    NSScreenSaverWindowLevel, NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindingRule,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSPoint, NSRect, NSSize};
use recast_capture::picker::{
    Highlight, Outcome, Picked, Picker, PickerCursor, PickerDisplay, PickerWindow, Point,
    point_from_appkit, rect_from_appkit,
};
use recast_project::Rect;

use super::{PickRequest, cursor};
use crate::appkit::{self, KeyPanel, TextStyle, color, fill, inset, ns_rect};

const KEY_ESCAPE: u16 = 53;
/// Gap between the menu bar (or the display's top edge) and the hint.
const HINT_TOP: f64 = 12.0;

pub type Done = Box<dyn FnOnce(Option<Picked>)>;

/// The crosshair's label as drawn: the display it is on, its rectangle in that view and
/// its text.
#[derive(Debug, Clone, PartialEq)]
struct ShownLabel {
    display: usize,
    rect: NSRect,
    text: String,
}

/// The picker's cursor as drawn: the display it is on and its rectangle in that view.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ShownCursor {
    display: usize,
    rect: NSRect,
}

struct Session {
    picker: Picker,
    main_height: f64,
    shown: Highlight,
    label: Option<ShownLabel>,
    label_style: TextStyle,
    hint: String,
    hint_style: TextStyle,
    /// The hint's rectangle in each display's view.
    hint_rects: Vec<NSRect>,
    /// The display showing the hint: the one under the pointer.
    hint_display: Option<usize>,
    panels: Vec<Retained<KeyPanel>>,
    views: Vec<Retained<OverlayView>>,
    camera: Retained<NSImage>,
    crosshair: Retained<NSImage>,
    cursor: Option<ShownCursor>,
    /// Times the system cursor was hidden, to show it as many times at the end.
    hides: usize,
    done: Option<Done>,
}

impl Session {
    /// Hides the system cursor, and hides it again when something else showed it: the
    /// Dock shows it when the pointer passes over it.
    fn keep_cursor_hidden(&mut self) {
        if self.hides == 0 || appkit::cursor_visible() {
            NSCursor::hide();
            self.hides += 1;
        }
    }

    fn cursor_image(&self) -> &Retained<NSImage> {
        match self.picker.cursor() {
            PickerCursor::Camera => &self.camera,
            PickerCursor::Crosshair => &self.crosshair,
        }
    }
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

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            self.take_key();
            self.update(|picker, at| picker.move_to(at));
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, _event: &NSEvent) {
            self.take_key();
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
                Outcome::Pending => refresh(&self.ivars().session),
            }
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, _event: &NSEvent) {
            self.finish(None);
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if event.keyCode() == KEY_ESCAPE {
                self.finish(None);
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

/// The crosshair's label for the picker's current state, placed in its view.
fn label_for(s: &Session) -> Option<ShownLabel> {
    let label = s.picker.pointer_label()?;
    let bounds = &s.picker.displays()[label.display].bounds;
    let size = label_size(&s.label_style, &label.text);
    let (x, y) = cursor::label_origin(
        (label.at.x, label.at.y),
        (size.width, size.height),
        (bounds.width, bounds.height),
    );
    Some(ShownLabel {
        display: label.display,
        rect: NSRect::new(NSPoint::new(x, y), size),
        text: label.text,
    })
}

/// The picker's cursor under the pointer, placed in the view of the display it is on.
fn cursor_for(s: &Session) -> Option<ShownCursor> {
    let at = pointer(s.main_height);
    let display = s.picker.display_index_at(at)?;
    let bounds = &s.picker.displays()[display].bounds;
    let hotspot = match s.picker.cursor() {
        PickerCursor::Camera => cursor::HOTSPOT,
        PickerCursor::Crosshair => cursor::CROSSHAIR_HOTSPOT,
    };
    Some(ShownCursor {
        display,
        rect: NSRect::new(
            NSPoint::new(at.x - bounds.x - hotspot.0, at.y - bounds.y - hotspot.1),
            NSSize::new(cursor::SIZE_POINTS, cursor::SIZE_POINTS),
        ),
    })
}

/// Brings the overlays in line with the picker and the pointer: redraws what changed.
/// AppKit is called with the session released, as it may call back into the views.
fn refresh(session: &Shared) {
    let (views, redraw, dirty) = {
        let mut s = session.borrow_mut();
        s.keep_cursor_hidden();
        let highlight = s.picker.highlight();
        let redraw = highlight != s.shown;
        s.shown = highlight;

        let label = label_for(&s);
        let mut dirty = Vec::new();
        let hint_display = s.picker.display_index_at(pointer(s.main_height));
        if hint_display != s.hint_display {
            dirty.extend(
                [s.hint_display, hint_display]
                    .into_iter()
                    .flatten()
                    .map(|d| (d, inset(s.hint_rects[d], -2.0, -2.0))),
            );
            s.hint_display = hint_display;
        }
        if label != s.label {
            dirty.extend(
                [&s.label, &label]
                    .into_iter()
                    .flatten()
                    .map(|l| (l.display, inset(l.rect, -2.0, -2.0))),
            );
            s.label = label;
        }

        let cursor = cursor_for(&s);
        if cursor != s.cursor {
            dirty.extend(
                [s.cursor, cursor]
                    .into_iter()
                    .flatten()
                    .map(|c| (c.display, inset(c.rect, -1.0, -1.0))),
            );
            s.cursor = cursor;
        }
        (s.views.clone(), redraw, dirty)
    };
    if redraw {
        for view in &views {
            view.setNeedsDisplay(true);
        }
    }
    for (display, rect) in dirty {
        if let Some(view) = views.get(display) {
            view.setNeedsDisplayInRect(rect);
        }
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
        refresh(session);
    }

    /// Makes this display's panel key when the pointer arrives on it.
    fn take_key(&self) {
        let Some(window) = self.window() else {
            return;
        };
        if !window.isKeyWindow() {
            window.makeKeyWindow();
            window.makeFirstResponder(Some(self));
        }
    }

    fn finish(&self, picked: Option<Picked>) {
        let mtm = self.mtm();
        let (panels, views, hides, done) = {
            let mut s = self.ivars().session.borrow_mut();
            (
                std::mem::take(&mut s.panels),
                std::mem::take(&mut s.views),
                std::mem::take(&mut s.hides),
                s.done.take(),
            )
        };
        for panel in &panels {
            panel.orderOut(None);
        }
        for _ in 0..hides {
            NSCursor::unhide();
        }
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
            Highlight::Region { bounds: region, .. } => {
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
        if session.hint_display == Some(self.ivars().display) {
            draw_pill(
                &session.hint_style,
                &session.hint,
                session.hint_rects[self.ivars().display],
            );
        }
        if let Some(label) = session
            .label
            .as_ref()
            .filter(|l| l.display == self.ivars().display)
        {
            draw_pill(&session.label_style, &label.text, label.rect);
        }
        if let Some(shown) = session.cursor.filter(|c| c.display == self.ivars().display) {
            session.cursor_image().drawInRect(shown.rect);
        }
    }
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
    crate::appkit::hide_from_capture(&panel);
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
    request: PickRequest,
    windows: Vec<PickerWindow>,
    own_pid: i32,
    done: Done,
) {
    let main_height = appkit::main_height(mtm);
    let mut displays = Vec::new();
    let mut frames = Vec::new();
    let mut top_insets = Vec::new();
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
        let visible = screen.visibleFrame();
        top_insets
            .push((frame.origin.y + frame.size.height) - (visible.origin.y + visible.size.height));
    }
    if displays.is_empty() {
        done(None);
        return;
    }

    let make_image =
        |(size_px, pixels): (usize, Vec<u8>)| appkit::image(size_px, &pixels, cursor::SIZE_POINTS);
    let camera = make_image(cursor::camera_pixels(2.0));
    let crosshair = make_image(cursor::crosshair_pixels(2.0));
    let mut picker = request.picker(displays, windows, own_pid);
    let at = pointer(main_height);
    picker.move_to(at);
    let key = picker.display_index_at(at).unwrap_or(0);
    let hint = request.hint();
    let hint_style = TextStyle::new(13.0, &NSColor::whiteColor());
    let hint_size = label_size(&hint_style, &hint);
    let hint_rects = frames
        .iter()
        .zip(&top_insets)
        .map(|(frame, top)| {
            NSRect::new(
                NSPoint::new((frame.size.width - hint_size.width) / 2.0, top + HINT_TOP),
                hint_size,
            )
        })
        .collect();
    let mut session = Session {
        hint_display: picker.display_index_at(at),
        hint,
        hint_style,
        hint_rects,
        shown: picker.highlight(),
        picker,
        main_height,
        label: None,
        label_style: TextStyle::new(12.0, &NSColor::whiteColor()),
        panels: Vec::new(),
        views: Vec::new(),
        camera,
        crosshair,
        cursor: None,
        hides: 0,
        done: Some(done),
    };
    session.label = label_for(&session);
    session.cursor = cursor_for(&session);
    let session: Shared = Rc::new(RefCell::new(session));

    for (index, frame) in frames.iter().enumerate() {
        let panel = make_panel(mtm, *frame);
        let view = OverlayView::new(mtm, index, session.clone(), frame.size);
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
        let mut s = session.borrow_mut();
        s.panels.push(panel);
        s.views.push(view);
    }

    let (panels, views) = {
        let s = session.borrow();
        (s.panels.clone(), s.views.clone())
    };
    for panel in &panels {
        panel.orderFrontRegardless();
    }
    panels[key].makeKeyWindow();
    panels[key].makeFirstResponder(Some(&views[key]));
    session.borrow_mut().keep_cursor_hidden();
}
