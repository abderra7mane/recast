use std::{
    ffi::c_void,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};

use cidre::{arc, cf, cg};
use recast_project::{EventKind, MouseButton};

use super::{now_host_ns, timebase};
use crate::{Error, InputRecord, InputSink, Result, event_time_ns};

unsafe extern "C" {
    // cidre declares this as returning f64; the C function returns a 64-bit integer.
    fn CGEventGetTimestamp(event: *const c_void) -> u64;
}

struct TapContext {
    sink: InputSink,
    tap: *mut cg::EventTap,
}

struct RunLoopHandle(arc::R<cf::RunLoop>);

// CFRunLoopStop may be called from any thread.
unsafe impl Send for RunLoopHandle {}

pub(super) struct EventTapThread {
    run_loop: RunLoopHandle,
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

const MASK: cg::EventMask = cg::EventType::MOUSE_MOVED.mask()
    | cg::EventType::LEFT_MOUSE_DOWN.mask()
    | cg::EventType::LEFT_MOUSE_UP.mask()
    | cg::EventType::LEFT_MOUSE_DRAGGED.mask()
    | cg::EventType::RIGHT_MOUSE_DOWN.mask()
    | cg::EventType::RIGHT_MOUSE_UP.mask()
    | cg::EventType::RIGHT_MOUSE_DRAGGED.mask()
    | cg::EventType::OHTER_MOUSE_DOWN.mask()
    | cg::EventType::OHTER_MOUSE_UP.mask()
    | cg::EventType::OTHER_MOUSE_DRAGGED.mask()
    | cg::EventType::SCROLL_WHEEL.mask();

fn kind(event_type: cg::EventType, event: &cg::Event) -> Option<EventKind> {
    let p = event.location();
    let (x, y) = (p.x, p.y);
    let click_count = || {
        event
            .field_i64(cg::EventField::MOUSE_EVENT_CLICK_STATE)
            .max(0) as u32
    };
    Some(match event_type {
        cg::EventType::MOUSE_MOVED => EventKind::Move { x, y },
        cg::EventType::LEFT_MOUSE_DRAGGED => EventKind::Drag {
            x,
            y,
            button: MouseButton::Left,
        },
        cg::EventType::RIGHT_MOUSE_DRAGGED => EventKind::Drag {
            x,
            y,
            button: MouseButton::Right,
        },
        cg::EventType::OTHER_MOUSE_DRAGGED => EventKind::Drag {
            x,
            y,
            button: MouseButton::Other,
        },
        cg::EventType::LEFT_MOUSE_DOWN => EventKind::Down {
            x,
            y,
            button: MouseButton::Left,
            click_count: click_count(),
        },
        cg::EventType::RIGHT_MOUSE_DOWN => EventKind::Down {
            x,
            y,
            button: MouseButton::Right,
            click_count: click_count(),
        },
        cg::EventType::OHTER_MOUSE_DOWN => EventKind::Down {
            x,
            y,
            button: MouseButton::Other,
            click_count: click_count(),
        },
        cg::EventType::LEFT_MOUSE_UP => EventKind::Up {
            x,
            y,
            button: MouseButton::Left,
        },
        cg::EventType::RIGHT_MOUSE_UP => EventKind::Up {
            x,
            y,
            button: MouseButton::Right,
        },
        cg::EventType::OHTER_MOUSE_UP => EventKind::Up {
            x,
            y,
            button: MouseButton::Other,
        },
        cg::EventType::SCROLL_WHEEL => EventKind::Scroll {
            x,
            y,
            dx: event.field_i64(cg::EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS2) as f64,
            dy: event.field_i64(cg::EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS1) as f64,
        },
        _ => return None,
    })
}

extern "C" fn callback(
    _proxy: *mut cg::EventTapProxy,
    event_type: cg::EventType,
    event: &mut cg::Event,
    ctx: *mut TapContext,
) -> Option<&cg::Event> {
    // SAFETY: the context outlives the run loop that calls this callback.
    let ctx = unsafe { &*ctx };
    if event_type == cg::EventType::TAP_DISABLED_BY_TIMEOUT
        || event_type == cg::EventType::TAP_DISABLED_BY_USER_INPUT
    {
        // SAFETY: the tap is alive for as long as its run loop runs.
        if let Some(tap) = unsafe { ctx.tap.as_mut() } {
            tap.set_enabled(true);
        }
        return Some(event);
    }
    if let Some(kind) = kind(event_type, event) {
        // SAFETY: `event` is a valid CGEventRef for the duration of the callback.
        let raw = unsafe { CGEventGetTimestamp(event as *const cg::Event as *const c_void) };
        let (numer, denom) = timebase();
        let host_ns = event_time_ns(raw, now_host_ns(), numer, denom);
        (ctx.sink)(InputRecord::Event { host_ns, kind });
    }
    Some(event)
}

impl EventTapThread {
    pub(super) fn start(sink: InputSink) -> Result<Self> {
        let (tx, rx) = mpsc::channel::<Result<RunLoopHandle>>();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = stop.clone();
        let thread = thread::Builder::new()
            .name("recast-event-tap".into())
            .spawn(move || {
                let mut ctx = Box::new(TapContext {
                    sink,
                    tap: std::ptr::null_mut(),
                });
                let Some(mut tap) = cg::EventTap::new(
                    cg::EventTapLocation::Hid,
                    cg::EventTapPlacement::TailAppend,
                    cg::EventTapOpts::LISTEN_ONLY,
                    MASK,
                    callback,
                    &mut *ctx,
                ) else {
                    let _ = tx.send(Err(Error::PermissionDenied));
                    return;
                };
                ctx.tap = &mut *tap;
                let Some(source) = tap.run_loop_src(0) else {
                    let _ = tx.send(Err(Error::Platform("cannot create run loop source".into())));
                    return;
                };
                let run_loop = cf::RunLoop::current();
                run_loop.add_src(&source, cf::RunLoopMode::common());
                tap.set_enabled(true);
                let _ = tx.send(Ok(RunLoopHandle(run_loop.retained())));
                while !stop_flag.load(Ordering::Acquire) {
                    let _ = cf::RunLoop::run_in_mode(cf::RunLoopMode::default(), 0.5, false);
                }
                tap.set_enabled(false);
                run_loop.remove_src(&source, cf::RunLoopMode::common());
                drop(ctx);
            })
            .map_err(|e| Error::Platform(e.to_string()))?;
        let run_loop = rx
            .recv()
            .map_err(|_| Error::Platform("event tap thread exited".into()))??;
        Ok(Self {
            run_loop,
            stop,
            thread,
        })
    }

    pub(super) fn stop(self) {
        self.stop.store(true, Ordering::Release);
        self.run_loop.0.stop();
        let _ = self.thread.join();
    }
}
