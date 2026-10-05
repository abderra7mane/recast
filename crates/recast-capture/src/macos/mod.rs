mod content;
mod mic;
mod recorder;
#[cfg(any(test, feature = "synthetic"))]
pub mod synthetic;
pub mod writer;

use std::{
    future::Future,
    sync::{Arc, OnceLock},
    task::{Context, Poll, Wake},
};

use cidre::{cm, mach};

use crate::{
    ActiveCapture, CaptureOptions, DisplayInfo, EventHandler, OutputFiles, Result, ScreenCapture,
    WindowInfo,
};

pub struct MacCapture;

impl ScreenCapture for MacCapture {
    fn displays(&self) -> Result<Vec<DisplayInfo>> {
        content::displays()
    }

    fn windows(&self) -> Result<Vec<WindowInfo>> {
        content::windows()
    }

    fn start(
        &self,
        options: &CaptureOptions,
        files: &OutputFiles,
        on_event: EventHandler,
    ) -> Result<Box<dyn ActiveCapture>> {
        Ok(Box::new(recorder::Recorder::start(
            options, files, on_event,
        )?))
    }
}

/// Runs a future to completion on the current thread. ScreenCaptureKit completion
/// handlers run on their own queues, so parking here does not deadlock.
pub(crate) fn block_on<F: Future>(fut: F) -> F::Output {
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

/// Converts a host-clock `CMTime` (the clock ScreenCaptureKit stamps samples with) to nanoseconds.
pub fn host_time_ns(time: cm::Time) -> u64 {
    ticks_to_ns(cm::Clock::convert_host_time_to_sys_units(time))
}

pub fn ticks_to_ns(ticks: u64) -> u64 {
    static TIMEBASE: OnceLock<(u32, u32)> = OnceLock::new();
    let (numer, denom) = *TIMEBASE.get_or_init(|| {
        let info = mach::TimeBaseInfo::new();
        (info.numer, info.denom)
    });
    (ticks as u128 * numer as u128 / denom as u128) as u64
}

pub fn now_host_ns() -> u64 {
    ticks_to_ns(mach::abs_time())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_clock_matches_mach_time() {
        let before = now_host_ns();
        let host = host_time_ns(cm::Clock::host_time_clock().time());
        let after = now_host_ns();
        assert!(before <= host + 1_000 && host <= after + 1_000);
    }

    #[test]
    fn block_on_runs_ready_future() {
        assert_eq!(block_on(async { 7 }), 7);
    }
}
