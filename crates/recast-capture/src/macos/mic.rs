use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use cidre::{
    arc,
    av::{self, capture::AudioDataOutputSampleBufDelegate},
    cm, define_obj_type, dispatch, ns, objc,
};

use super::{host_time_ns, recorder::AudioSlot, writer::platform};
use crate::{Error, EventHandler, Result, Track};

/// Captures the default microphone with AVCaptureSession, which works on macOS 14
/// (ScreenCaptureKit only captures the microphone from macOS 15).
pub(super) struct MicCapture {
    session: arc::R<av::CaptureSession>,
    shared: Arc<MicShared>,
    _output: arc::R<av::capture::AudioDataOutput>,
    _delegate: arc::R<MicSink>,
    _queue: arc::R<dispatch::Queue>,
}

unsafe impl Send for MicCapture {}

struct MicShared {
    slot: Mutex<Option<AudioSlot>>,
    clock: Mutex<Option<arc::R<cm::Clock>>>,
    on_event: EventHandler,
}

impl MicShared {
    fn to_host_ns(&self, time: cm::Time) -> u64 {
        let clock = self.clock.lock().expect("clock lock");
        match clock.as_ref() {
            Some(clock) => host_time_ns(clock.convert_time_to(time, cm::Clock::host_time_clock())),
            None => host_time_ns(time),
        }
    }
}

pub(super) struct MicInner {
    shared: Arc<MicShared>,
}

define_obj_type!(
    MicSink(ns::Id) + av::capture::AudioDataOutputSampleBufDelegateImpl,
    MicInner,
    RECAST_MIC_SINK
);

impl AudioDataOutputSampleBufDelegate for MicSink {}

#[objc::add_methods]
impl av::capture::AudioDataOutputSampleBufDelegateImpl for MicSink {
    extern "C" fn impl_capture_output_did_output_sample_buf_from_connection(
        &mut self,
        _cmd: Option<&objc::Sel>,
        _output: &av::CaptureOutput,
        sample_buf: &cm::SampleBuf,
        _connection: &av::CaptureConnection,
    ) {
        let shared = &self.inner().shared;
        if let Some(slot) = shared.slot.lock().expect("mic lock").as_mut() {
            slot.append(
                sample_buf,
                Track::Mic,
                |t| shared.to_host_ns(t),
                &shared.on_event,
            );
        }
    }
}

impl MicCapture {
    pub(super) fn start(path: PathBuf, on_event: EventHandler) -> Result<Self> {
        let status = av::CaptureDevice::authorization_status_for_media_type(av::MediaType::audio())
            .map_err(|e| Error::Platform(format!("{e:?}")))?;
        if status != av::AuthorizationStatus::Authorized {
            return Err(Error::Platform("microphone permission is missing".into()));
        }
        let device = av::CaptureDevice::default_with_media(av::MediaType::audio())
            .ok_or_else(|| Error::NotFound("microphone".into()))?;
        let input = av::CaptureDeviceInput::with_device(&device)
            .map_err(|e| platform("cannot open microphone", e))?;

        let shared = Arc::new(MicShared {
            slot: Mutex::new(Some(AudioSlot::new(path))),
            clock: Mutex::new(None),
            on_event,
        });
        let delegate = MicSink::with(MicInner {
            shared: shared.clone(),
        });
        let queue = dispatch::Queue::serial_with_ar_pool();
        let mut output = av::capture::AudioDataOutput::new();
        output.set_sample_buf_delegate(Some(delegate.as_ref()), Some(&queue));

        let mut session = av::CaptureSession::new();
        let mut added = Ok(());
        session.configure(|s| {
            if !s.can_add_input(&input) {
                added = Err(Error::Platform("cannot add microphone input".into()));
                return;
            }
            s.add_input(&input);
            if !s.can_add_output(&output) {
                added = Err(Error::Platform("cannot add microphone output".into()));
                return;
            }
            s.add_output(&output);
        });
        added?;
        *shared.clock.lock().expect("clock lock") = session.sync_clock().map(|c| c.retained());
        session.start_running();

        Ok(Self {
            session,
            shared,
            _output: output,
            _delegate: delegate,
            _queue: queue,
        })
    }

    pub(super) fn stop(mut self) -> Result<()> {
        self.session.stop_running();
        let slot = self.shared.slot.lock().expect("mic lock").take();
        slot.map(AudioSlot::finish).transpose().map(|_| ())
    }
}
