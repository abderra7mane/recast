mod cursor;
pub mod permissions;
mod tap;

use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
    task::{Context, Poll, Wake},
};

use cidre::mach;

use crate::{ActiveInput, InputCapture, InputSink, Result};

pub struct MacInput;

impl InputCapture for MacInput {
    fn start(
        &self,
        cursors_dir: PathBuf,
        relative_dir: String,
        sink: InputSink,
    ) -> Result<Box<dyn ActiveInput>> {
        let tap = if cidre::cg::event::access::listen_preflight() {
            Some(tap::EventTapThread::start(sink.clone())?)
        } else {
            None
        };
        let cursor = cursor::CursorSampler::start(cursors_dir, relative_dir, sink);
        Ok(Box::new(MacActiveInput { tap, cursor }))
    }
}

struct MacActiveInput {
    tap: Option<tap::EventTapThread>,
    cursor: cursor::CursorSampler,
}

impl ActiveInput for MacActiveInput {
    fn listening(&self) -> bool {
        self.tap.is_some()
    }

    fn stop(self: Box<Self>) {
        if let Some(tap) = self.tap {
            tap.stop();
        }
        self.cursor.stop();
    }
}

pub(crate) fn timebase() -> (u32, u32) {
    static TIMEBASE: OnceLock<(u32, u32)> = OnceLock::new();
    *TIMEBASE.get_or_init(|| {
        let info = mach::TimeBaseInfo::new();
        (info.numer, info.denom)
    })
}

/// Current host-clock time in nanoseconds.
pub fn now_host_ns() -> u64 {
    let (numer, denom) = timebase();
    (mach::abs_time() as u128 * numer as u128 / denom as u128) as u64
}

pub(crate) fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Arc::new(Unpark(std::thread::current())).into();
    let mut cx = Context::from_waker(&waker);
    let mut fut = std::pin::pin!(fut);
    loop {
        if let Poll::Ready(out) = fut.as_mut().poll(&mut cx) {
            return out;
        }
        std::thread::park();
    }
}
