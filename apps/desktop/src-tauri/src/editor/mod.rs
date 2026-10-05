//! The editor backend: one session per open project, with a preview stream,
//! playback, autosave and the events-derived data the UI shows.

pub mod audio;
pub mod autosave;
pub mod commands;
pub mod player;
pub mod protocol;
pub mod server;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use recast_project::{Bundle, EditSettings, MouseButton, Project, ZoomSegment};
use recast_zoom::{Click, Timeline, auto_segments};
use serde::{Deserialize, Serialize};
use specta::Type;

use autosave::Autosave;
use player::{Command, PlaybackStatus, Player, PlayerInit, PlayerStats};
use server::PreviewServer;

const AUTOSAVE_DELAY: Duration = Duration::from_millis(600);

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClickMarker {
    pub t_ms: f64,
    pub button: MouseButton,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EditorInit {
    pub bundle_path: String,
    pub project: Project,
    /// The saved edits, with every value in range.
    pub settings: EditSettings,
    pub duration_ms: f64,
    /// Segments auto zoom generates at the saved zoom level.
    pub auto_segments: Vec<ZoomSegment>,
    /// Button presses inside the recorded area.
    pub clicks: Vec<ClickMarker>,
    /// WebSocket URL of the preview stream, including its token.
    pub preview_url: String,
    /// Suggested path for an export.
    pub export_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EditorStatus {
    pub playing: bool,
    pub position_ms: f64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewStats {
    pub frames: u32,
    pub render_ms: f64,
    pub last_seek_ms: f64,
    pub width: u32,
    pub height: u32,
    pub dropped: u32,
    /// Whether audio plays on an output device; `None` while it is being opened.
    pub audio_output: Option<bool>,
}

pub struct EditorSession {
    bundle: Bundle,
    project: Mutex<Project>,
    clicks: Vec<Click>,
    duration_ms: f64,
    player: Player,
    server: Arc<PreviewServer>,
    autosave: Autosave,
    seeks: OrderGate,
    edits: OrderGate,
}

/// Lets through only requests newer than the newest one seen. Commands can arrive
/// out of order, and an older seek or edit must not replace a newer one.
#[derive(Default)]
struct OrderGate(Mutex<Option<u32>>);

impl OrderGate {
    fn admit(&self, seq: u32, send: impl FnOnce()) -> bool {
        let mut last = self.0.lock().expect("order lock");
        if last.is_some_and(|last| seq <= last) {
            return false;
        }
        *last = Some(seq);
        send();
        true
    }

    fn reset(&self) {
        *self.0.lock().expect("order lock") = None;
    }
}

impl EditorSession {
    pub fn open(path: &Path) -> Result<Self, String> {
        let bundle = Bundle::open(path).map_err(|e| e.to_string())?;
        if bundle.is_unfinished() {
            return Err("this recording did not finish; recover it first".into());
        }
        let project = bundle.load_project().map_err(|e| e.to_string())?;
        let events = Arc::new(bundle.load_events().map_err(|e| e.to_string())?);
        let duration_ms = project.recording.duration_ms;
        let settings = project.edits.sanitized(duration_ms);
        let clicks = Timeline::new(&events, &project.recording.bounds, &settings, duration_ms)
            .clicks()
            .to_vec();
        let server = Arc::new(PreviewServer::start().map_err(|e| e.to_string())?);
        let player = Player::start(PlayerInit {
            bundle_dir: bundle.path().to_path_buf(),
            project: project.clone(),
            events,
            settings: settings.clone(),
            server: server.clone(),
        })?;
        let autosave = Autosave::start(bundle.clone(), AUTOSAVE_DELAY);
        Ok(Self {
            bundle,
            project: Mutex::new(Project {
                edits: settings,
                ..project
            }),
            clicks,
            duration_ms,
            player,
            server,
            autosave,
            seeks: OrderGate::default(),
            edits: OrderGate::default(),
        })
    }

    pub fn path(&self) -> &Path {
        self.bundle.path()
    }

    pub fn settings(&self) -> EditSettings {
        self.project.lock().expect("project lock").edits.clone()
    }

    pub fn auto_segments(&self, settings: &EditSettings) -> Vec<ZoomSegment> {
        auto_segments(&self.clicks, settings.zoom.level, self.duration_ms)
    }

    pub fn init(&self) -> EditorInit {
        let project = self.project.lock().expect("project lock").clone();
        let settings = project.edits.clone();
        EditorInit {
            bundle_path: self.bundle.path().display().to_string(),
            auto_segments: self.auto_segments(&settings),
            clicks: self
                .clicks
                .iter()
                .filter(|c| c.down && c.on_screen())
                .map(|c| ClickMarker {
                    t_ms: c.t_ms,
                    button: c.button,
                })
                .collect(),
            duration_ms: self.duration_ms,
            preview_url: self.server.url(),
            export_path: self
                .bundle
                .path()
                .with_extension("mp4")
                .display()
                .to_string(),
            settings,
            project,
        }
    }

    /// Applies edits to the preview and schedules a save. Returns the segments auto
    /// zoom generates for them, or `None` when a newer `seq` was already applied.
    pub fn set_settings(&self, settings: EditSettings, seq: u32) -> Option<Vec<ZoomSegment>> {
        let settings = settings.sanitized(self.duration_ms);
        let segments = self.auto_segments(&settings);
        let applied = self.edits.admit(seq, || {
            let mut project = self.project.lock().expect("project lock");
            if project.edits != settings {
                project.edits = settings.clone();
                self.autosave.save(project.clone());
                self.player.send(Command::Settings(Box::new(settings)));
            }
        });
        applied.then_some(segments)
    }

    pub fn play(&self) {
        self.player.send(Command::Play);
    }

    pub fn pause(&self) {
        self.player.send(Command::Pause);
    }

    /// Seeks to `t_ms`. `seq` increases with every seek the UI makes; seeks that
    /// arrive after a newer one are ignored.
    pub fn seek(&self, t_ms: f64, seq: u32) {
        if t_ms.is_finite() {
            self.seeks
                .admit(seq, || self.player.send(Command::Seek(t_ms)));
        }
    }

    /// Starts new seek and edit sequences, for a page that was (re)loaded.
    pub fn reset_sequences(&self) {
        self.seeks.reset();
        self.edits.reset();
    }

    pub fn set_loop(&self, looping: bool) {
        self.player.send(Command::Loop(looping));
    }

    pub fn resize(&self, width: u32, height: u32) {
        self.player.send(Command::Resize(width, height));
    }

    pub fn status(&self) -> EditorStatus {
        let PlaybackStatus {
            playing,
            position_ms,
        } = self.player.status();
        EditorStatus {
            playing,
            position_ms,
            error: self.player.error(),
        }
    }

    pub fn stats(&self) -> PreviewStats {
        let PlayerStats {
            frames,
            render_ms,
            last_seek_ms,
            width,
            height,
            audio_output,
        } = self.player.stats();
        PreviewStats {
            frames: frames as u32,
            render_ms,
            last_seek_ms,
            width,
            height,
            dropped: self.server.dropped() as u32,
            audio_output,
        }
    }

    pub fn flush(&self) -> Result<(), String> {
        self.autosave.flush()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub path: String,
    pub name: String,
    pub created_at_unix_ms: f64,
    pub duration_ms: f64,
    pub width: u32,
    pub height: u32,
}

/// Finished projects under `root`, newest first.
pub fn list_projects(root: &Path) -> Vec<ProjectSummary> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut projects: Vec<ProjectSummary> = entries
        .filter_map(|entry| {
            let path: PathBuf = entry.ok()?.path();
            if path.extension()? != recast_project::BUNDLE_EXTENSION {
                return None;
            }
            let bundle = Bundle::open(&path).ok()?;
            if bundle.is_unfinished() {
                return None;
            }
            let project = bundle.load_project().ok()?;
            Some(ProjectSummary {
                path: path.display().to_string(),
                name: bundle.name(),
                created_at_unix_ms: project.created_at_unix_ms,
                duration_ms: project.recording.duration_ms,
                width: project.recording.width,
                height: project.recording.height,
            })
        })
        .collect();
    projects.sort_by(|a, b| b.created_at_unix_ms.total_cmp(&a.created_at_unix_ms));
    projects
}

#[cfg(test)]
pub(crate) mod test_support {
    use recast_project::{
        CaptureSource, EVENTS_FILE, Project, Recording, Rect, SCHEMA_VERSION, VideoTrack,
    };

    pub fn project(name: &str, created_at_unix_ms: f64) -> Project {
        Project {
            version: SCHEMA_VERSION,
            name: name.into(),
            created_at_unix_ms,
            recording: Recording {
                source: CaptureSource::Display { display_id: 1 },
                bounds: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 320.0,
                    height: 200.0,
                },
                width: 640,
                height: 400,
                scale_factor: 2.0,
                fps: 60,
                duration_ms: 3_000.0,
                video: VideoTrack {
                    file: "screen.mp4".into(),
                    codec: "hevc".into(),
                    duration_ms: 3_000.0,
                },
                system_audio: None,
                mic: None,
                events_file: EVENTS_FILE.into(),
                cursors_dir: "cursors".into(),
                recovered: false,
            },
            edits: Default::default(),
        }
    }

    /// A 3 s, 64×40 recording without audio or input events.
    #[cfg(feature = "synthetic")]
    pub fn fixture(root: &std::path::Path) -> std::path::PathBuf {
        let path = root.join("Fixture.recast");
        std::fs::create_dir(&path).unwrap();
        use cidre::cm;
        use recast_capture::macos::{synthetic, writer::MediaWriter};
        use std::{thread::sleep, time::Duration};

        let base = cm::Time::new(1_000, 1);
        let mut video = MediaWriter::hevc(&path.join("screen.mp4"), 64, 40, 60).unwrap();
        for i in 0..180 {
            let pts = base.add(cm::Time::new(i, 60));
            let frame =
                synthetic::video_frame(64, 40, i as u64, pts, cm::Time::new(1, 60)).unwrap();
            while !video.is_ready() {
                sleep(Duration::from_millis(1));
            }
            video.append(&frame).unwrap();
        }
        assert!(
            video
                .finish_at(Some(base.add(cm::Time::new(3, 1))))
                .unwrap()
        );
        std::fs::write(
            path.join(recast_project::PROJECT_FILE),
            serde_json::to_vec(&project("Fixture", 0.0)).unwrap(),
        )
        .unwrap();
        recast_project::Bundle::open(&path)
            .unwrap()
            .save_events(&recast_project::EventLog::default())
            .unwrap();
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_project(root: &Path, name: &str, created: f64) -> PathBuf {
        let path = root.join(format!("{name}.recast"));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(
            path.join(recast_project::PROJECT_FILE),
            serde_json::to_vec(&test_support::project(name, created)).unwrap(),
        )
        .unwrap();
        path
    }

    #[test]
    fn requests_arriving_late_are_ignored_until_reset() {
        let gate = OrderGate::default();
        assert!(gate.admit(1, || {}));
        assert!(gate.admit(3, || {}));
        assert!(!gate.admit(2, || panic!("stale request sent")));
        assert!(!gate.admit(3, || panic!("repeated request sent")));
        gate.reset();
        assert!(gate.admit(1, || {}));
    }

    #[test]
    fn lists_finished_projects_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        write_project(dir.path(), "Old", 1_000.0);
        write_project(dir.path(), "New", 2_000.0);
        let crashed = write_project(dir.path(), "Crashed", 3_000.0);
        std::fs::write(crashed.join(recast_project::MARKER_FILE), b"{}").unwrap();
        std::fs::create_dir(dir.path().join("notes")).unwrap();
        std::fs::create_dir(dir.path().join("Empty.recast")).unwrap();

        let names: Vec<_> = list_projects(dir.path())
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["New", "Old"]);
        assert!(list_projects(&dir.path().join("missing")).is_empty());
    }
}

#[cfg(all(test, feature = "synthetic"))]
mod playback_tests {
    use std::{
        net::TcpStream,
        thread::sleep,
        time::{Duration, Instant},
    };

    use recast_project::Trim;
    use tungstenite::{Message, WebSocket, stream::MaybeTlsStream};

    use super::{protocol::FrameHeader, *};

    type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

    fn connect(url: &str) -> Socket {
        let (socket, _) = tungstenite::connect(url).unwrap();
        if let MaybeTlsStream::Plain(stream) = socket.get_ref() {
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
        }
        socket
    }

    fn next_frame(socket: &mut Socket) -> FrameHeader {
        loop {
            if let Message::Binary(bytes) = socket.read().unwrap() {
                return FrameHeader::decode(&bytes).unwrap();
            }
        }
    }

    fn first_playing_frame(socket: &mut Socket) -> FrameHeader {
        loop {
            let frame = next_frame(socket);
            if frame.playing {
                return frame;
            }
        }
    }

    /// Reads frames from the first playing one until playback stops; returns the
    /// first frame, how many were playing and the frame shown when it stopped.
    fn play_through(socket: &mut Socket) -> (FrameHeader, usize, FrameHeader) {
        let first = first_playing_frame(socket);
        let started = Instant::now();
        let mut playing = 1;
        loop {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "playback never stopped"
            );
            let frame = next_frame(socket);
            if !frame.playing {
                return (first, playing, frame);
            }
            playing += 1;
        }
    }

    fn settle(session: &EditorSession) {
        sleep(Duration::from_millis(150));
        assert!(!session.status().playing);
    }

    #[test]
    fn play_after_reaching_the_end_starts_at_the_playhead() {
        let dir = tempfile::tempdir().unwrap();
        let session = EditorSession::open(&test_support::fixture(dir.path())).unwrap();
        let mut socket = connect(&session.init().preview_url);

        session.seek(2_500.0, 1);
        session.play();
        let (_, played, end) = play_through(&mut socket);
        assert!(played > 5);
        assert!((end.t_ms - 3_000.0).abs() < 1e-6, "{}", end.t_ms);
        settle(&session);

        session.seek(1_000.0, 2);
        session.play();
        let (first, played, _) = play_through(&mut socket);
        assert!(
            (1_000.0..1_100.0).contains(&first.t_ms),
            "first frame at {}",
            first.t_ms
        );
        assert!(played > 60, "only {played} frames");
    }

    #[test]
    fn older_edits_arriving_late_are_dropped_and_newer_ones_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = test_support::fixture(dir.path());
        let session = EditorSession::open(&path).unwrap();
        let mut newer = session.settings();
        newer.background.padding = 0.3;
        let mut older = session.settings();
        older.background.padding = 0.1;

        assert!(session.set_settings(newer.clone(), 2).is_some());
        assert!(session.set_settings(older, 1).is_none());
        assert_eq!(session.settings().background.padding, 0.3);
        session.flush().unwrap();
        let saved = Bundle::open(&path).unwrap().load_project().unwrap();
        assert_eq!(saved.edits.background.padding, 0.3);

        session.reset_sequences();
        assert!(session.set_settings(newer, 1).is_some());
    }

    #[test]
    fn play_inside_a_shorter_trim_runs_to_its_end() {
        let dir = tempfile::tempdir().unwrap();
        let session = EditorSession::open(&test_support::fixture(dir.path())).unwrap();
        let mut socket = connect(&session.init().preview_url);

        session.seek(2_500.0, 1);
        session.play();
        play_through(&mut socket);
        settle(&session);

        let mut settings = session.settings();
        settings.trim = Trim {
            start_ms: 0.0,
            end_ms: Some(1_500.0),
        };
        session.set_settings(settings, 1);
        session.seek(500.0, 2);
        session.play();
        let (first, played, end) = play_through(&mut socket);
        assert!(
            (500.0..600.0).contains(&first.t_ms),
            "first frame at {}",
            first.t_ms
        );
        assert!(played > 30, "only {played} frames");
        assert!((end.t_ms - 1_500.0).abs() < 1e-6, "{}", end.t_ms);
    }
}
