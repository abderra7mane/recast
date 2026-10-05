use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

use recast_capture::{
    ActiveCapture, CaptureEvent, CaptureInfo, CaptureOptions, CaptureTarget, OutputFiles,
    ScreenCapture,
};
use recast_input::{ActiveInput, InputCapture, InputRecord};
use recast_project::{
    AudioTrack, Bundle, CURSORS_DIR, CaptureSource, EVENTS_FILE, LogRecord, MIC_FILE, Project,
    RECORD_LOG_FILE, RecordLogWriter, Recording, SCHEMA_VERSION, SYSTEM_AUDIO_FILE, Track,
    VIDEO_FILE, VideoTrack, now_unix_ms,
};
use serde::{Deserialize, Serialize};
use specta::Type;

pub const DEFAULT_FPS: u32 = 60;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecordingRequest {
    pub target: CaptureTarget,
    pub system_audio: bool,
    pub mic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    pub bundle_path: String,
    pub elapsed_ms: f64,
    pub width: u32,
    pub height: u32,
    /// False when Input Monitoring is missing, so clicks are not recorded.
    pub input_events: bool,
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FinishedRecording {
    pub bundle_path: String,
    pub project: Project,
    /// Problems that did not prevent saving, such as the capture stopping on its own.
    pub warnings: Vec<String>,
}

type Log = Arc<Mutex<Option<RecordLogWriter>>>;

pub struct Session {
    bundle: Bundle,
    capture: Box<dyn ActiveCapture>,
    input: Box<dyn ActiveInput>,
    log: Log,
    problems: Arc<Mutex<Vec<String>>>,
    started: Instant,
}

fn append(log: &Log, problems: &Mutex<Vec<String>>, record: LogRecord) {
    let mut guard = log.lock().expect("log lock");
    if let Some(writer) = guard.as_mut()
        && let Err(e) = writer.append(&record)
    {
        problems
            .lock()
            .expect("problems lock")
            .push(format!("event log: {e}"));
        *guard = None;
    }
}

fn source(target: &CaptureTarget, info: &CaptureInfo) -> CaptureSource {
    match target {
        CaptureTarget::Display { display_id } => CaptureSource::Display {
            display_id: *display_id,
        },
        CaptureTarget::Window { window_id } => CaptureSource::Window {
            window_id: *window_id,
            title: info.window_title.clone(),
            app_name: info.app_name.clone(),
        },
        CaptureTarget::Region { display_id, rect } => CaptureSource::Region {
            display_id: *display_id,
            rect: *rect,
        },
    }
}

fn initial_project(name: String, request: &RecordingRequest, info: &CaptureInfo) -> Project {
    let audio = |enabled: bool, file: &str| {
        enabled.then(|| AudioTrack {
            file: file.into(),
            offset_ms: 0.0,
            duration_ms: 0.0,
        })
    };
    Project {
        version: SCHEMA_VERSION,
        name,
        created_at_unix_ms: now_unix_ms(),
        recording: Recording {
            source: source(&request.target, info),
            bounds: info.bounds,
            width: info.width,
            height: info.height,
            scale_factor: info.scale_factor,
            fps: info.fps,
            duration_ms: 0.0,
            video: VideoTrack {
                file: VIDEO_FILE.into(),
                codec: info.codec.clone(),
                duration_ms: 0.0,
            },
            system_audio: audio(request.system_audio, SYSTEM_AUDIO_FILE),
            mic: audio(request.mic, MIC_FILE),
            events_file: EVENTS_FILE.into(),
            cursors_dir: CURSORS_DIR.into(),
            recovered: false,
        },
        edits: Default::default(),
    }
}

/// After a failed finalize: a bundle without video is useless and is deleted; any
/// other bundle is handed to recovery so the user can retry or discard it.
fn give_up(bundle: &Bundle, error: &recast_project::Error) {
    if matches!(error, recast_project::Error::NoVideo) {
        let _ = std::fs::remove_dir_all(bundle.path());
    } else {
        let _ = bundle.abandon();
    }
}

pub fn default_name() -> String {
    chrono::Local::now()
        .format("Recording %Y-%m-%d at %H.%M.%S")
        .to_string()
}

impl Session {
    pub fn start(
        root: &Path,
        name: &str,
        request: &RecordingRequest,
        capture: &dyn ScreenCapture,
        input: &dyn InputCapture,
    ) -> Result<Self, String> {
        let bundle = Bundle::create(root, name).map_err(|e| e.to_string())?;
        let cleanup = |e: String| {
            let _ = std::fs::remove_dir_all(bundle.path());
            e
        };
        let log: Log = Arc::new(Mutex::new(Some(
            RecordLogWriter::create(&bundle.file(RECORD_LOG_FILE))
                .map_err(|e| cleanup(e.to_string()))?,
        )));
        let problems = Arc::new(Mutex::new(Vec::new()));

        let input_sink = {
            let (log, problems) = (log.clone(), problems.clone());
            Arc::new(move |record: InputRecord| {
                let record = match record {
                    InputRecord::Event { host_ns, kind } => LogRecord::Input { host_ns, kind },
                    InputRecord::Shape(shape) => LogRecord::CursorShape(shape),
                };
                append(&log, &problems, record);
            })
        };
        let active_input = input
            .start(
                bundle.file(CURSORS_DIR),
                CURSORS_DIR.to_string(),
                input_sink,
            )
            .map_err(|e| cleanup(e.to_string()))?;

        let capture_handler = {
            let (log, problems) = (log.clone(), problems.clone());
            Arc::new(move |event: CaptureEvent| match event {
                CaptureEvent::TrackStarted { track, host_ns } => {
                    let track = match track {
                        recast_capture::Track::Video => Track::Video,
                        recast_capture::Track::SystemAudio => Track::SystemAudio,
                        recast_capture::Track::Mic => Track::Mic,
                    };
                    append(&log, &problems, LogRecord::TrackStarted { track, host_ns });
                }
                CaptureEvent::Failed { message } => {
                    problems.lock().expect("problems lock").push(message);
                }
            })
        };
        let options = CaptureOptions {
            target: request.target.clone(),
            fps: DEFAULT_FPS,
            system_audio: request.system_audio,
            mic: request.mic,
        };
        let files = OutputFiles {
            video: bundle.file(VIDEO_FILE),
            system_audio: bundle.file(SYSTEM_AUDIO_FILE),
            mic: bundle.file(MIC_FILE),
        };
        let active_capture = match capture.start(&options, &files, capture_handler) {
            Ok(c) => c,
            Err(e) => {
                active_input.stop();
                return Err(cleanup(e.to_string()));
            }
        };

        let project = initial_project(bundle.name(), request, active_capture.info());
        if let Err(e) = bundle.save_project(&project) {
            problems
                .lock()
                .expect("problems lock")
                .push(format!("project.json: {e}"));
        }

        Ok(Self {
            bundle,
            capture: active_capture,
            input: active_input,
            log,
            problems,
            started: Instant::now(),
        })
    }

    pub fn path(&self) -> PathBuf {
        self.bundle.path().to_path_buf()
    }

    pub fn status(&self) -> RecordingStatus {
        let info = self.capture.info();
        RecordingStatus {
            bundle_path: self.bundle.path().display().to_string(),
            elapsed_ms: self.started.elapsed().as_secs_f64() * 1000.0,
            width: info.width,
            height: info.height,
            input_events: self.input.listening(),
            problems: self.problems.lock().expect("problems lock").clone(),
        }
    }

    /// Stops and saves the recording. A capture error does not lose the bundle: the
    /// media is trimmed to what was fully written and the error comes back as a warning.
    pub fn stop(self) -> Result<FinishedRecording, String> {
        self.input.stop();
        let stopped = self.capture.stop();
        self.log.lock().expect("log lock").take();
        let mut warnings = self.problems.lock().expect("problems lock").clone();
        if let Err(e) = &stopped {
            warnings.push(format!("capture did not stop cleanly: {e}"));
        }
        let project = match self.bundle.finalize_with(stopped.is_err(), false) {
            Ok(project) => project,
            Err(e) => {
                give_up(&self.bundle, &e);
                return Err(match &stopped {
                    Ok(()) => e.to_string(),
                    Err(stop) => format!("{e} (capture: {stop})"),
                });
            }
        };
        Ok(FinishedRecording {
            bundle_path: self.bundle.path().display().to_string(),
            project,
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use recast_capture::{DisplayInfo, EventHandler, WindowInfo};
    use recast_project::Rect;

    use super::*;

    struct FakeCapture;
    struct FakeActive(CaptureInfo);

    impl ScreenCapture for FakeCapture {
        fn displays(&self) -> recast_capture::Result<Vec<DisplayInfo>> {
            Ok(vec![])
        }
        fn windows(&self) -> recast_capture::Result<Vec<WindowInfo>> {
            Ok(vec![])
        }
        fn start(
            &self,
            _options: &CaptureOptions,
            _files: &OutputFiles,
            on_event: EventHandler,
        ) -> recast_capture::Result<Box<dyn ActiveCapture>> {
            on_event(CaptureEvent::TrackStarted {
                track: recast_capture::Track::Video,
                host_ns: 1_000_000_000,
            });
            Ok(Box::new(FakeActive(CaptureInfo {
                width: 640,
                height: 400,
                scale_factor: 2.0,
                bounds: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 320.0,
                    height: 200.0,
                },
                fps: 60,
                codec: "hevc".into(),
                window_title: None,
                app_name: None,
            })))
        }
    }

    impl ActiveCapture for FakeActive {
        fn info(&self) -> &CaptureInfo {
            &self.0
        }
        fn stop(self: Box<Self>) -> recast_capture::Result<()> {
            Ok(())
        }
    }

    struct FakeInput;
    struct FakeInputActive;

    impl InputCapture for FakeInput {
        fn start(
            &self,
            _dir: PathBuf,
            _relative: String,
            sink: recast_input::InputSink,
        ) -> recast_input::Result<Box<dyn ActiveInput>> {
            sink(InputRecord::Event {
                host_ns: 1_250_000_000,
                kind: recast_project::EventKind::Move { x: 1.0, y: 2.0 },
            });
            Ok(Box::new(FakeInputActive))
        }
    }

    impl ActiveInput for FakeInputActive {
        fn listening(&self) -> bool {
            true
        }
        fn stop(self: Box<Self>) {}
    }

    #[test]
    fn failed_start_removes_bundle() {
        struct Failing;
        impl ScreenCapture for Failing {
            fn displays(&self) -> recast_capture::Result<Vec<DisplayInfo>> {
                Ok(vec![])
            }
            fn windows(&self) -> recast_capture::Result<Vec<WindowInfo>> {
                Ok(vec![])
            }
            fn start(
                &self,
                _: &CaptureOptions,
                _: &OutputFiles,
                _: EventHandler,
            ) -> recast_capture::Result<Box<dyn ActiveCapture>> {
                Err(recast_capture::Error::PermissionDenied)
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let request = RecordingRequest {
            target: CaptureTarget::Display { display_id: 1 },
            system_audio: false,
            mic: false,
        };
        let err = Session::start(dir.path(), "Nope", &request, &Failing, &FakeInput)
            .err()
            .unwrap();
        assert!(err.contains("permission"));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    struct NoFrames;

    impl ScreenCapture for NoFrames {
        fn displays(&self) -> recast_capture::Result<Vec<DisplayInfo>> {
            Ok(vec![])
        }
        fn windows(&self) -> recast_capture::Result<Vec<WindowInfo>> {
            Ok(vec![])
        }
        fn start(
            &self,
            options: &CaptureOptions,
            files: &OutputFiles,
            _: EventHandler,
        ) -> recast_capture::Result<Box<dyn ActiveCapture>> {
            FakeCapture.start(options, files, Arc::new(|_| {}))
        }
    }

    fn display_request() -> RecordingRequest {
        RecordingRequest {
            target: CaptureTarget::Display { display_id: 1 },
            system_audio: false,
            mic: false,
        }
    }

    #[test]
    fn stop_without_frames_removes_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::start(
            dir.path(),
            "Quick",
            &display_request(),
            &NoFrames,
            &FakeInput,
        )
        .unwrap();
        let path = session.path();
        let err = session.stop().unwrap_err();
        assert!(err.contains("no video frames"), "{err}");
        assert!(!path.exists());
    }

    #[test]
    fn stop_with_broken_video_hands_bundle_to_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::start(
            dir.path(),
            "Broken",
            &display_request(),
            &FakeCapture,
            &FakeInput,
        )
        .unwrap();
        let path = session.path();
        std::fs::write(path.join(VIDEO_FILE), b"not a movie").unwrap();
        assert!(session.stop().is_err());
        let listed = recast_project::list_unfinished(dir.path()).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].problem.is_some());
        assert!(Bundle::open_crashed(&path).is_ok());
    }

    #[test]
    fn session_writes_project_and_log() {
        let dir = tempfile::tempdir().unwrap();
        let request = RecordingRequest {
            target: CaptureTarget::Region {
                display_id: 1,
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 320.0,
                    height: 200.0,
                },
            },
            system_audio: true,
            mic: false,
        };
        let session =
            Session::start(dir.path(), "Take", &request, &FakeCapture, &FakeInput).unwrap();
        let status = session.status();
        assert_eq!(status.width, 640);
        assert!(status.input_events);

        let bundle = Bundle::open(&session.path()).unwrap();
        let project = bundle.load_project().unwrap();
        assert_eq!(project.recording.scale_factor, 2.0);
        assert!(project.recording.system_audio.is_some());
        assert!(matches!(
            project.recording.source,
            CaptureSource::Region { .. }
        ));

        let records = recast_project::read_record_log(&bundle.file(RECORD_LOG_FILE)).unwrap();
        assert_eq!(records.len(), 2);
        let starts = recast_project::TrackStarts::from_records(&records);
        assert_eq!(starts.video, Some(1_000_000_000));
        let log = recast_project::compile_event_log(&records, 1_000_000_000);
        assert_eq!(log.events[0].t_ms, 250.0);
    }

    #[cfg(feature = "synthetic")]
    #[test]
    fn stop_failure_still_saves_with_warning() {
        use crate::synthetic_capture::SyntheticCapture;

        struct StopFails(SyntheticCapture);
        struct StopFailsActive(Box<dyn ActiveCapture>);

        impl ScreenCapture for StopFails {
            fn displays(&self) -> recast_capture::Result<Vec<DisplayInfo>> {
                Ok(vec![])
            }
            fn windows(&self) -> recast_capture::Result<Vec<WindowInfo>> {
                Ok(vec![])
            }
            fn start(
                &self,
                options: &CaptureOptions,
                files: &OutputFiles,
                on_event: EventHandler,
            ) -> recast_capture::Result<Box<dyn ActiveCapture>> {
                Ok(Box::new(StopFailsActive(
                    self.0.start(options, files, on_event)?,
                )))
            }
        }

        impl ActiveCapture for StopFailsActive {
            fn info(&self) -> &CaptureInfo {
                self.0.info()
            }
            fn stop(self: Box<Self>) -> recast_capture::Result<()> {
                self.0.stop()?;
                Err(recast_capture::Error::Platform(
                    "stream already stopped".into(),
                ))
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let request = RecordingRequest {
            target: CaptureTarget::Display { display_id: 1 },
            system_audio: false,
            mic: false,
        };
        let capture = StopFails(SyntheticCapture {
            width: 320,
            height: 180,
        });
        let session = Session::start(dir.path(), "Flaky", &request, &capture, &FakeInput).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(600));
        let finished = session.stop().unwrap();
        assert!(finished.project.recording.video.duration_ms >= 500.0);
        assert!(
            finished
                .warnings
                .iter()
                .any(|w| w.contains("stream already stopped"))
        );
        let bundle = Bundle::open(std::path::Path::new(&finished.bundle_path)).unwrap();
        assert!(!bundle.is_unfinished());
    }
}
