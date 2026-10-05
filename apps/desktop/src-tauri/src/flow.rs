//! The recording flow after a target is picked: the countdown, then the recording, which
//! the control bar stops, restarts or cancels.

use std::sync::{
    Mutex, MutexGuard,
    atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::recording::{FinishedRecording, RecordingRequest, RecordingStatus, Session};

/// Starts a recording session for a request.
pub type Start<'a> = &'a dyn Fn(&RecordingRequest) -> Result<Session, String>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Phase {
    Idle,
    Picking,
    Countdown,
    /// The countdown is over and capture is starting.
    Starting,
    #[serde(rename_all = "camelCase")]
    Recording {
        elapsed_ms: f64,
    },
    Stopping,
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Picking,
    Countdown {
        request: RecordingRequest,
        id: u64,
    },
    /// `Session::start` with this id runs without the lock held.
    Starting {
        id: u64,
    },
    Recording {
        request: RecordingRequest,
        session: Session,
    },
    Stopping,
}

/// What follows a pick or a restart.
#[derive(Debug)]
pub enum Next {
    /// The countdown with this id runs; `countdown_done` starts the recording.
    Countdown(u64),
    Recording(RecordingStatus),
    /// Stop or Cancel came while capture was starting; the new recording was deleted.
    Cancelled,
}

#[derive(Debug)]
pub enum Stopped {
    Nothing,
    /// Stopped during the countdown or while capture was starting, so nothing was saved.
    Cancelled,
    Finished(Box<FinishedRecording>),
}

#[derive(Default)]
pub struct Flow {
    state: Mutex<State>,
    next_id: AtomicU64,
}

impl Flow {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn phase(&self) -> Phase {
        match &*self.lock() {
            State::Idle => Phase::Idle,
            State::Picking => Phase::Picking,
            State::Countdown { .. } => Phase::Countdown,
            State::Starting { .. } => Phase::Starting,
            State::Recording { session, .. } => Phase::Recording {
                elapsed_ms: session.status().elapsed_ms,
            },
            State::Stopping => Phase::Stopping,
        }
    }

    pub fn is_recording(&self) -> bool {
        matches!(*self.lock(), State::Recording { .. })
    }

    /// Waits up to `timeout` for a recording being saved to finish; false when it didn't.
    pub fn wait_while_stopping(&self, timeout: std::time::Duration) -> bool {
        let started = std::time::Instant::now();
        while matches!(*self.lock(), State::Stopping) {
            if started.elapsed() >= timeout {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        true
    }

    pub fn status(&self) -> Option<RecordingStatus> {
        match &*self.lock() {
            State::Recording { session, .. } => Some(session.status()),
            _ => None,
        }
    }

    /// Claims the flow for picking a target; fails while anything else runs.
    pub fn begin_pick(&self) -> Result<(), String> {
        let mut state = self.lock();
        match *state {
            State::Idle => {
                *state = State::Picking;
                Ok(())
            }
            State::Recording { .. } => Err("A recording is already running.".into()),
            _ => Err("Recast is busy with another recording.".into()),
        }
    }

    pub fn pick_cancelled(&self) {
        let mut state = self.lock();
        if matches!(*state, State::Picking) {
            *state = State::Idle;
        }
    }

    pub fn picked(
        &self,
        request: RecordingRequest,
        countdown: bool,
        start: Start,
    ) -> Result<Next, String> {
        let state = self.lock();
        if !matches!(*state, State::Picking) {
            return Err("The recording was cancelled.".into());
        }
        self.begin(state, request, countdown, start)
    }

    /// Starts the countdown, or capture when the countdown is off. Capture starts with
    /// the lock released, so `phase` and Stop never wait for it.
    fn begin(
        &self,
        mut state: MutexGuard<'_, State>,
        request: RecordingRequest,
        countdown: bool,
        start: Start,
    ) -> Result<Next, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if countdown {
            *state = State::Countdown { request, id };
            return Ok(Next::Countdown(id));
        }
        *state = State::Starting { id };
        drop(state);
        let started = start(&request);
        let mut state = self.lock();
        let current = matches!(*state, State::Starting { id: current } if current == id);
        match started {
            Ok(session) if current => {
                let status = session.status();
                *state = State::Recording { request, session };
                Ok(Next::Recording(status))
            }
            Ok(session) => {
                drop(state);
                session.discard();
                Ok(Next::Cancelled)
            }
            Err(e) => {
                if current {
                    *state = State::Idle;
                }
                Err(e)
            }
        }
    }

    /// Ends countdown `id` and starts recording. `None` when that countdown was
    /// cancelled or replaced, or the recording was stopped while starting.
    pub fn countdown_done(&self, id: u64, start: Start) -> Result<Option<RecordingStatus>, String> {
        let state = self.lock();
        let request = match &*state {
            State::Countdown {
                request,
                id: current,
            } if *current == id => request.clone(),
            _ => return Ok(None),
        };
        match self.begin(state, request, false, start)? {
            Next::Recording(status) => Ok(Some(status)),
            Next::Cancelled => Ok(None),
            Next::Countdown(_) => unreachable!("countdown is off"),
        }
    }

    /// Takes the running session out, leaving the flow in `Stopping`.
    fn take_session(&self) -> Option<(RecordingRequest, Session)> {
        let mut state = self.lock();
        match std::mem::replace(&mut *state, State::Stopping) {
            State::Recording { request, session } => Some((request, session)),
            other => {
                *state = other;
                None
            }
        }
    }

    fn set_idle(&self) {
        *self.lock() = State::Idle;
    }

    /// Stops and saves the recording, or cancels the countdown or a starting capture.
    pub fn stop(&self) -> Result<Stopped, String> {
        if self.cancel_before_recording() {
            return Ok(Stopped::Cancelled);
        }
        let Some((_, session)) = self.take_session() else {
            return Ok(Stopped::Nothing);
        };
        let finished = session.stop();
        self.set_idle();
        finished.map(|f| Stopped::Finished(Box::new(f)))
    }

    /// Deletes the recording and starts over with the same target and options.
    pub fn restart(&self, countdown: bool, start: Start) -> Result<Option<Next>, String> {
        let Some((request, session)) = self.take_session() else {
            return Ok(None);
        };
        session.discard();
        self.begin(self.lock(), request, countdown, start).map(Some)
    }

    /// Cancels the countdown or deletes the recording, also one still starting. False
    /// when none of these was running.
    pub fn cancel(&self) -> bool {
        if self.cancel_before_recording() {
            return true;
        }
        let Some((_, session)) = self.take_session() else {
            return false;
        };
        session.discard();
        self.set_idle();
        true
    }

    /// Ends a countdown or a starting capture; `begin` deletes what the capture recorded.
    fn cancel_before_recording(&self) -> bool {
        let mut state = self.lock();
        if matches!(*state, State::Countdown { .. } | State::Starting { .. }) {
            *state = State::Idle;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use recast_capture::CaptureTarget;

    use super::*;
    use crate::recording::fakes::{FakeCapture, FakeInput};

    fn request() -> RecordingRequest {
        RecordingRequest {
            target: CaptureTarget::Display { display_id: 1 },
            system_audio: false,
            mic: false,
        }
    }

    fn fake_start(root: &Path) -> impl Fn(&RecordingRequest) -> Result<Session, String> + '_ {
        move |request| Session::start(root, "Take", request, &FakeCapture, &FakeInput)
    }

    fn bundles(root: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(root)
            .map(|entries| entries.flatten().map(|e| e.path()).collect())
            .unwrap_or_default()
    }

    fn recording_path(flow: &Flow) -> PathBuf {
        PathBuf::from(flow.status().expect("recording").bundle_path)
    }

    #[test]
    fn countdown_then_recording() {
        let dir = tempfile::tempdir().unwrap();
        let start = fake_start(dir.path());
        let flow = Flow::default();
        flow.begin_pick().unwrap();
        assert_eq!(flow.phase(), Phase::Picking);
        assert!(flow.begin_pick().is_err(), "one flow at a time");

        let Next::Countdown(id) = flow.picked(request(), true, &start).unwrap() else {
            panic!("expected a countdown");
        };
        assert_eq!(flow.phase(), Phase::Countdown);
        assert!(bundles(dir.path()).is_empty(), "nothing records yet");
        assert!(flow.countdown_done(id + 1, &start).unwrap().is_none());

        let status = flow.countdown_done(id, &start).unwrap().unwrap();
        assert!(Path::new(&status.bundle_path).exists());
        assert!(matches!(flow.phase(), Phase::Recording { .. }));
        assert!(flow.countdown_done(id, &start).unwrap().is_none(), "once");
        assert!(flow.begin_pick().is_err());
    }

    #[test]
    fn no_countdown_records_right_away() {
        let dir = tempfile::tempdir().unwrap();
        let start = fake_start(dir.path());
        let flow = Flow::default();
        flow.begin_pick().unwrap();
        assert!(matches!(
            flow.picked(request(), false, &start).unwrap(),
            Next::Recording(_)
        ));
        assert_eq!(bundles(dir.path()).len(), 1);
    }

    #[test]
    fn a_cancelled_pick_frees_the_flow() {
        let flow = Flow::default();
        flow.begin_pick().unwrap();
        flow.pick_cancelled();
        assert_eq!(flow.phase(), Phase::Idle);
        flow.begin_pick().unwrap();
    }

    #[test]
    fn a_failed_start_returns_to_idle() {
        let flow = Flow::default();
        flow.begin_pick().unwrap();
        let failing = |_: &RecordingRequest| -> Result<Session, String> { Err("denied".into()) };
        assert_eq!(
            flow.picked(request(), false, &failing).unwrap_err(),
            "denied"
        );
        assert_eq!(flow.phase(), Phase::Idle);
    }

    #[test]
    fn stopping_the_countdown_records_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let start = fake_start(dir.path());
        let flow = Flow::default();
        flow.begin_pick().unwrap();
        let Next::Countdown(id) = flow.picked(request(), true, &start).unwrap() else {
            panic!("expected a countdown");
        };
        assert!(matches!(flow.stop().unwrap(), Stopped::Cancelled));
        assert!(flow.countdown_done(id, &start).unwrap().is_none());
        assert_eq!(flow.phase(), Phase::Idle);
        assert!(bundles(dir.path()).is_empty());
        assert!(matches!(flow.stop().unwrap(), Stopped::Nothing));
    }

    #[test]
    fn restart_discards_the_bundle_and_counts_down_again() {
        let dir = tempfile::tempdir().unwrap();
        let start = fake_start(dir.path());
        let flow = Flow::default();
        flow.begin_pick().unwrap();
        flow.picked(request(), false, &start).unwrap();
        let first = recording_path(&flow);

        let Some(Next::Countdown(id)) = flow.restart(true, &start).unwrap() else {
            panic!("expected a countdown");
        };
        assert!(!first.exists());
        assert!(bundles(dir.path()).is_empty());
        flow.countdown_done(id, &start).unwrap().unwrap();
        let second = recording_path(&flow);
        assert!(second.exists());
        assert_eq!(bundles(dir.path()), vec![second]);
    }

    #[test]
    fn restart_without_countdown_records_again() {
        let dir = tempfile::tempdir().unwrap();
        let start = fake_start(dir.path());
        let flow = Flow::default();
        flow.begin_pick().unwrap();
        flow.picked(request(), false, &start).unwrap();
        let first = recording_path(&flow);
        std::fs::write(first.join("marker"), b"first take").unwrap();
        let Some(Next::Recording(status)) = flow.restart(false, &start).unwrap() else {
            panic!("expected a recording");
        };
        let second = PathBuf::from(&status.bundle_path);
        assert!(!second.join("marker").exists(), "the first take is gone");
        assert_eq!(bundles(dir.path()), vec![second]);
        assert!(flow.restart(false, &start).unwrap().is_some());
    }

    #[test]
    fn restart_needs_a_recording() {
        let dir = tempfile::tempdir().unwrap();
        let start = fake_start(dir.path());
        assert!(Flow::default().restart(true, &start).unwrap().is_none());
    }

    #[test]
    fn cancel_discards_the_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let start = fake_start(dir.path());
        let flow = Flow::default();
        flow.begin_pick().unwrap();
        flow.picked(request(), false, &start).unwrap();
        let path = recording_path(&flow);
        assert!(flow.cancel());
        assert!(!path.exists());
        assert_eq!(flow.phase(), Phase::Idle);
        assert!(!flow.cancel());
    }

    #[test]
    fn is_recording_only_while_recording() {
        let dir = tempfile::tempdir().unwrap();
        let start = fake_start(dir.path());
        let flow = Flow::default();
        assert!(!flow.is_recording());
        flow.begin_pick().unwrap();
        flow.picked(request(), true, &start).unwrap();
        assert!(!flow.is_recording(), "counting down");
        flow.cancel();
        flow.begin_pick().unwrap();
        flow.picked(request(), false, &start).unwrap();
        assert!(flow.is_recording());
        flow.cancel();
        assert!(!flow.is_recording());
    }

    #[test]
    fn quitting_waits_for_a_recording_being_saved() {
        use std::{sync::Arc, time::Duration};

        let flow = Arc::new(Flow::default());
        assert!(
            flow.wait_while_stopping(Duration::ZERO),
            "nothing to wait for"
        );

        *flow.lock() = State::Stopping;
        assert!(!flow.wait_while_stopping(Duration::from_millis(50)));
        let saver = {
            let flow = flow.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                flow.set_idle();
            })
        };
        assert!(flow.wait_while_stopping(Duration::from_secs(5)));
        assert_eq!(flow.phase(), Phase::Idle);
        saver.join().unwrap();
    }

    /// A start that runs `during` while capture is starting, as a Stop or Cancel from
    /// another thread would.
    fn start_with<'a>(
        root: &'a Path,
        during: impl Fn() + 'a,
    ) -> impl Fn(&RecordingRequest) -> Result<Session, String> + 'a {
        move |request| {
            during();
            Session::start(root, "Take", request, &FakeCapture, &FakeInput)
        }
    }

    #[test]
    fn stop_while_starting_deletes_the_new_recording() {
        let dir = tempfile::tempdir().unwrap();
        let flow = Flow::default();
        let start = start_with(dir.path(), || {
            assert_eq!(flow.phase(), Phase::Starting, "phase doesn't wait");
            assert!(matches!(flow.stop().unwrap(), Stopped::Cancelled));
            assert_eq!(flow.phase(), Phase::Idle);
        });
        flow.begin_pick().unwrap();
        let Next::Countdown(id) = flow.picked(request(), true, &start).unwrap() else {
            panic!("expected a countdown");
        };
        assert!(flow.countdown_done(id, &start).unwrap().is_none());
        assert_eq!(flow.phase(), Phase::Idle);
        assert!(bundles(dir.path()).is_empty());
    }

    #[test]
    fn cancel_while_starting_deletes_the_new_recording() {
        let dir = tempfile::tempdir().unwrap();
        let flow = Flow::default();
        let start = start_with(dir.path(), || assert!(flow.cancel()));
        flow.begin_pick().unwrap();
        assert!(matches!(
            flow.picked(request(), false, &start).unwrap(),
            Next::Cancelled
        ));
        assert_eq!(flow.phase(), Phase::Idle);
        assert!(bundles(dir.path()).is_empty());
    }

    #[test]
    fn a_new_pick_after_cancelling_a_start_keeps_its_state() {
        let dir = tempfile::tempdir().unwrap();
        let flow = Flow::default();
        let start = start_with(dir.path(), || {
            assert!(flow.begin_pick().is_err(), "busy while starting");
            assert!(flow.cancel());
            flow.begin_pick().unwrap();
        });
        flow.begin_pick().unwrap();
        assert!(matches!(
            flow.picked(request(), false, &start).unwrap(),
            Next::Cancelled
        ));
        assert_eq!(flow.phase(), Phase::Picking, "the new pick is untouched");
        assert!(bundles(dir.path()).is_empty());
    }

    #[test]
    fn a_failed_start_after_a_cancel_leaves_the_new_state() {
        let flow = Flow::default();
        let failing = |_: &RecordingRequest| -> Result<Session, String> {
            assert!(flow.cancel());
            flow.begin_pick().unwrap();
            Err("denied".into())
        };
        flow.begin_pick().unwrap();
        assert!(flow.picked(request(), false, &failing).is_err());
        assert_eq!(flow.phase(), Phase::Picking);
    }

    #[test]
    fn cancel_ends_the_countdown() {
        let dir = tempfile::tempdir().unwrap();
        let start = fake_start(dir.path());
        let flow = Flow::default();
        flow.begin_pick().unwrap();
        flow.picked(request(), true, &start).unwrap();
        assert!(flow.cancel());
        assert_eq!(flow.phase(), Phase::Idle);
    }

    #[cfg(feature = "synthetic")]
    #[test]
    fn stop_finalizes_the_bundle() {
        use crate::{synthetic_capture::SyntheticCapture, synthetic_input::SyntheticInput};

        let dir = tempfile::tempdir().unwrap();
        let capture = SyntheticCapture {
            width: 320,
            height: 180,
        };
        let input = SyntheticInput {
            width: 320.0,
            height: 180.0,
        };
        let start = |request: &RecordingRequest| {
            Session::start(dir.path(), "Take", request, &capture, &input)
        };
        let flow = Flow::default();
        flow.begin_pick().unwrap();
        flow.picked(request(), false, &start).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(600));
        let Stopped::Finished(finished) = flow.stop().unwrap() else {
            panic!("expected a finished recording");
        };
        assert_eq!(flow.phase(), Phase::Idle);
        let bundle = recast_project::Bundle::open(Path::new(&finished.bundle_path)).unwrap();
        assert!(!bundle.is_unfinished());
        assert!(finished.project.recording.duration_ms >= 500.0);
    }
}
