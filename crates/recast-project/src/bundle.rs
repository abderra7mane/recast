use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    AudioTrack, Error, EventLog, Project, Result, SCHEMA_VERSION, TrackStarts, compile_event_log,
    mp4, ns_delta_ms, read_record_log,
};

pub const BUNDLE_EXTENSION: &str = "recast";
pub const PROJECT_FILE: &str = "project.json";
pub const EVENTS_FILE: &str = "events.msgpack";
pub const RECORD_LOG_FILE: &str = "events.log";
pub const CURSORS_DIR: &str = "cursors";
pub const VIDEO_FILE: &str = "screen.mp4";
pub const SYSTEM_AUDIO_FILE: &str = "system-audio.m4a";
pub const MIC_FILE: &str = "mic.m4a";
pub const MARKER_FILE: &str = "recording.lock";

/// `~/Movies/Recast`
pub fn default_root() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join("Movies").join("Recast")
}

pub fn now_unix_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

/// Present while a recording is being written; its absence means the bundle is finished.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    pub pid: u32,
    /// Start time of the recording process, so a reused PID is not mistaken for it.
    #[serde(default)]
    pub process_started_us: Option<u64>,
    pub started_at_unix_ms: f64,
    /// Set when the recorder gave up on the bundle while still running.
    #[serde(default)]
    pub abandoned: bool,
}

impl Marker {
    fn for_current_process() -> Self {
        let pid = std::process::id();
        Self {
            pid,
            process_started_us: process_start_us(pid),
            started_at_unix_ms: now_unix_ms(),
            abandoned: false,
        }
    }

    /// Whether the process that wrote this marker is still running.
    pub fn recorder_alive(&self) -> bool {
        if self.abandoned {
            return false;
        }
        match (self.process_started_us, process_start_us(self.pid)) {
            (Some(expected), actual) => actual == Some(expected),
            (None, _) => pid_exists(self.pid),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UnfinishedBundle {
    pub path: String,
    pub name: String,
    pub started_at_unix_ms: f64,
    /// Why recovery cannot work, e.g. the recorder stopped before the first video fragment.
    pub problem: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Bundle {
    path: PathBuf,
}

impl Bundle {
    /// Creates `<root>/<name>.recast`, adding a numeric suffix if it already exists,
    /// and marks it as unfinished.
    pub fn create(root: &Path, name: &str) -> Result<Self> {
        fs::create_dir_all(root)?;
        let mut candidate = root.join(format!("{name}.{BUNDLE_EXTENSION}"));
        let mut n = 2;
        while candidate.exists() {
            candidate = root.join(format!("{name} {n}.{BUNDLE_EXTENSION}"));
            n += 1;
        }
        fs::create_dir(&candidate)?;
        let bundle = Self { path: candidate };
        fs::create_dir(bundle.file(CURSORS_DIR))?;
        write_atomic(
            &bundle.file(MARKER_FILE),
            &serde_json::to_vec_pretty(&Marker::for_current_process())?,
        )?;
        Ok(bundle)
    }

    pub fn open(path: &Path) -> Result<Self> {
        if !path.join(PROJECT_FILE).is_file() {
            return Err(Error::NotABundle(path.display().to_string()));
        }
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn name(&self) -> String {
        self.path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    pub fn file(&self, relative: &str) -> PathBuf {
        self.path.join(relative)
    }

    pub fn save_project(&self, project: &Project) -> Result<()> {
        write_atomic(
            &self.file(PROJECT_FILE),
            &serde_json::to_vec_pretty(project)?,
        )
    }

    pub fn load_project(&self) -> Result<Project> {
        let project: Project = serde_json::from_slice(&fs::read(self.file(PROJECT_FILE))?)?;
        if project.version > SCHEMA_VERSION {
            return Err(Error::UnsupportedVersion(project.version));
        }
        Ok(project)
    }

    pub fn save_events(&self, events: &EventLog) -> Result<()> {
        write_atomic(&self.file(EVENTS_FILE), &events.to_bytes()?)
    }

    pub fn load_events(&self) -> Result<EventLog> {
        EventLog::from_bytes(&fs::read(self.file(EVENTS_FILE))?)
    }

    pub fn marker(&self) -> Option<Marker> {
        serde_json::from_slice(&fs::read(self.file(MARKER_FILE)).ok()?).ok()
    }

    pub fn is_unfinished(&self) -> bool {
        self.file(MARKER_FILE).exists()
    }

    /// Hands an unfinished bundle over to recovery while this process keeps running,
    /// e.g. after finalizing failed.
    pub fn abandon(&self) -> Result<()> {
        let mut marker = self.marker().unwrap_or_else(Marker::for_current_process);
        marker.abandoned = true;
        write_atomic(
            &self.file(MARKER_FILE),
            &serde_json::to_vec_pretty(&marker)?,
        )
    }

    /// Opens a bundle left unfinished by a recorder that is no longer running.
    pub fn open_crashed(path: &Path) -> Result<Self> {
        let bundle = Self {
            path: path.to_path_buf(),
        };
        let is_bundle =
            path.extension().and_then(|e| e.to_str()) == Some(BUNDLE_EXTENSION) && path.is_dir();
        if !is_bundle || !bundle.is_unfinished() {
            return Err(Error::NotABundle(path.display().to_string()));
        }
        if let Some(marker) = bundle.marker()
            && marker.recorder_alive()
        {
            return Err(Error::StillRecording(marker.pid));
        }
        Ok(bundle)
    }

    /// Checks, without changing anything, that `finalize` has what it needs.
    pub fn check_recoverable(&self) -> Result<()> {
        if !self.file(PROJECT_FILE).is_file() {
            return Err(Error::NoProject);
        }
        let project = self.load_project()?;
        let records = read_record_log(&self.file(RECORD_LOG_FILE)).unwrap_or_default();
        TrackStarts::from_records(&records)
            .video
            .ok_or(Error::NoVideo)?;
        mp4::scan_file(&self.file(&project.recording.video.file))?
            .track(mp4::TrackKind::Video)
            .ok_or(Error::NoVideo)?;
        Ok(())
    }

    /// Turns the raw recording output into the final bundle: compiles the event log,
    /// measures every media file and updates `project.json`. Used both after a normal
    /// stop and when recovering a crashed recording.
    pub fn finalize(&self, recovered: bool) -> Result<Project> {
        self.finalize_with(recovered, recovered)
    }

    /// `repair` trims partial media data first, for files whose writer did not finish.
    pub fn finalize_with(&self, repair: bool, recovered: bool) -> Result<Project> {
        let mut project = self.load_project()?;
        let records = match read_record_log(&self.file(RECORD_LOG_FILE)) {
            Ok(records) => records,
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        let starts = TrackStarts::from_records(&records);
        let video_start = starts.video.ok_or(Error::NoVideo)?;

        let recording = &mut project.recording;
        let video = repair_or_scan(&self.file(&recording.video.file), repair)?;
        let video_ms = video
            .track(mp4::TrackKind::Video)
            .map(mp4::TrackSummary::duration_ms)
            .ok_or(Error::NoVideo)?;
        recording.video.duration_ms = video_ms;
        recording.duration_ms = video_ms;

        recording.system_audio = self.audio_track(
            recording.system_audio.take(),
            starts.system_audio,
            video_start,
            repair,
        );
        recording.mic = self.audio_track(recording.mic.take(), starts.mic, video_start, repair);
        recording.recovered = recovered;

        self.save_events(&compile_event_log(&records, video_start))?;
        self.save_project(&project)?;
        remove_if_exists(&self.file(RECORD_LOG_FILE))?;
        remove_if_exists(&self.file(MARKER_FILE))?;
        Ok(project)
    }

    fn audio_track(
        &self,
        track: Option<AudioTrack>,
        start_ns: Option<u64>,
        video_start_ns: u64,
        repair: bool,
    ) -> Option<AudioTrack> {
        let track = track?;
        let path = self.file(&track.file);
        let measured = start_ns.and_then(|start| {
            let summary = repair_or_scan(&path, repair).ok()?;
            let audio = summary.track(mp4::TrackKind::Audio)?;
            Some(AudioTrack {
                offset_ms: ns_delta_ms(video_start_ns, start),
                duration_ms: audio.duration_ms(),
                ..track
            })
        });
        if measured.is_none() {
            let _ = fs::remove_file(&path);
        }
        measured
    }
}

fn repair_or_scan(path: &Path, repair: bool) -> Result<mp4::Summary> {
    if repair {
        mp4::repair_file(path)
    } else {
        mp4::scan_file(path)
    }
}

/// Bundles under `root` that still carry a recording marker and whose recorder
/// process is gone, each with the reason it cannot be recovered, if any.
pub fn list_unfinished(root: &Path) -> Result<Vec<UnfinishedBundle>> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut found = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some(BUNDLE_EXTENSION) {
            continue;
        }
        let bundle = Bundle { path };
        if !bundle.is_unfinished() {
            continue;
        }
        let marker = bundle.marker();
        if marker.as_ref().is_some_and(Marker::recorder_alive) {
            continue;
        }
        found.push(UnfinishedBundle {
            path: bundle.path.display().to_string(),
            name: bundle.name(),
            started_at_unix_ms: marker.map(|m| m.started_at_unix_ms).unwrap_or(0.0),
            problem: bundle.check_recoverable().err().map(|e| e.to_string()),
        });
    }
    found.sort_by(|a, b| b.started_at_unix_ms.total_cmp(&a.started_at_unix_ms));
    Ok(found)
}

/// Recovers a bundle left behind by a crash: trims partial media data and finalizes
/// metadata from whatever was written.
pub fn recover(path: &Path) -> Result<Project> {
    Bundle::open_crashed(path)?;
    Bundle::open(path)?.finalize(true)
}

fn pid_exists(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 only checks whether the process exists.
    let rc = unsafe { libc::kill(pid, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Process start time in microseconds since the Unix epoch.
#[cfg(target_os = "macos")]
fn process_start_us(pid: u32) -> Option<u64> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    // SAFETY: `info` is a writable proc_bsdinfo of `size` bytes.
    let written = unsafe {
        libc::proc_pidinfo(
            i32::try_from(pid).ok()?,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    (written == size).then(|| info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec)
}

#[cfg(not(target_os = "macos"))]
fn process_start_us(_pid: u32) -> Option<u64> {
    None
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CaptureSource, EventKind, LogRecord, MouseButton, RecordLogWriter, Recording, Rect, Track,
        VideoTrack,
        test_support::{AUDIO, VIDEO, fragmented_file},
    };

    fn project(name: &str, mic: bool) -> Project {
        Project {
            version: SCHEMA_VERSION,
            name: name.into(),
            created_at_unix_ms: 1_700_000_000_000.0,
            recording: Recording {
                source: CaptureSource::Region {
                    display_id: 1,
                    rect: Rect {
                        x: 10.0,
                        y: 20.0,
                        width: 300.0,
                        height: 200.0,
                    },
                },
                bounds: Rect {
                    x: 10.0,
                    y: 20.0,
                    width: 300.0,
                    height: 200.0,
                },
                width: 600,
                height: 400,
                scale_factor: 2.0,
                fps: 60,
                duration_ms: 0.0,
                video: VideoTrack {
                    file: VIDEO_FILE.into(),
                    codec: "hevc".into(),
                    duration_ms: 0.0,
                },
                system_audio: Some(AudioTrack {
                    file: SYSTEM_AUDIO_FILE.into(),
                    offset_ms: 0.0,
                    duration_ms: 0.0,
                }),
                mic: mic.then(|| AudioTrack {
                    file: MIC_FILE.into(),
                    offset_ms: 0.0,
                    duration_ms: 0.0,
                }),
                events_file: EVENTS_FILE.into(),
                cursors_dir: CURSORS_DIR.into(),
                recovered: false,
            },
        }
    }

    fn write_log(bundle: &Bundle, records: &[LogRecord]) {
        let mut writer = RecordLogWriter::create(&bundle.file(RECORD_LOG_FILE)).unwrap();
        for r in records {
            writer.append(r).unwrap();
        }
    }

    #[test]
    fn project_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = Bundle::create(dir.path(), "Demo").unwrap();
        let original = project("Demo", true);
        bundle.save_project(&original).unwrap();
        let reopened = Bundle::open(bundle.path()).unwrap();
        assert_eq!(reopened.load_project().unwrap(), original);

        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(bundle.file(PROJECT_FILE)).unwrap()).unwrap();
        assert_eq!(json["version"], SCHEMA_VERSION);
        assert_eq!(json["recording"]["source"]["kind"], "region");
        assert_eq!(json["recording"]["scaleFactor"], 2.0);
    }

    #[test]
    fn rejects_newer_schema() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = Bundle::create(dir.path(), "Future").unwrap();
        let mut p = project("Future", false);
        p.version = SCHEMA_VERSION + 1;
        bundle.save_project(&p).unwrap();
        assert!(matches!(
            bundle.load_project(),
            Err(Error::UnsupportedVersion(_))
        ));
    }

    #[test]
    fn create_picks_unique_names() {
        let dir = tempfile::tempdir().unwrap();
        let a = Bundle::create(dir.path(), "Take").unwrap();
        let b = Bundle::create(dir.path(), "Take").unwrap();
        assert_eq!(a.name(), "Take");
        assert_eq!(b.name(), "Take 2");
        assert!(a.file(CURSORS_DIR).is_dir());
        assert!(a.is_unfinished());
    }

    #[test]
    fn finalize_fills_metadata_and_clears_marker() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = Bundle::create(dir.path(), "Clean").unwrap();
        bundle.save_project(&project("Clean", true)).unwrap();
        fs::write(bundle.file(VIDEO_FILE), fragmented_file(&VIDEO, 2, 60)).unwrap();
        fs::write(
            bundle.file(SYSTEM_AUDIO_FILE),
            fragmented_file(&AUDIO, 1, 47),
        )
        .unwrap();
        write_log(
            &bundle,
            &[
                LogRecord::TrackStarted {
                    track: Track::Video,
                    host_ns: 10_000_000,
                },
                LogRecord::TrackStarted {
                    track: Track::SystemAudio,
                    host_ns: 12_500_000,
                },
                LogRecord::Input {
                    host_ns: 30_000_000,
                    kind: EventKind::Down {
                        x: 50.0,
                        y: 60.0,
                        button: MouseButton::Left,
                        click_count: 1,
                    },
                },
            ],
        );

        let p = bundle.finalize(false).unwrap();
        let r = &p.recording;
        assert_eq!(r.video.duration_ms, 2000.0);
        assert_eq!(r.duration_ms, 2000.0);
        let sys = r.system_audio.as_ref().unwrap();
        assert_eq!(sys.offset_ms, 2.5);
        assert!((sys.duration_ms - 47.0 * 1024.0 / 48.0).abs() < 1e-9);
        assert!(r.mic.is_none(), "mic never started, so it is dropped");
        assert!(!r.recovered);
        assert!(!bundle.is_unfinished());
        assert!(!bundle.file(RECORD_LOG_FILE).exists());

        let events = bundle.load_events().unwrap();
        assert_eq!(events.events.len(), 1);
        assert_eq!(events.events[0].t_ms, 20.0);
        assert_eq!(bundle.load_project().unwrap(), p);
    }

    #[test]
    fn finalize_without_frames_fails() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = Bundle::create(dir.path(), "Empty").unwrap();
        bundle.save_project(&project("Empty", false)).unwrap();
        assert!(matches!(bundle.finalize(false), Err(Error::NoVideo)));
        assert!(bundle.is_unfinished());
    }

    #[test]
    fn lists_only_unfinished_bundles_of_dead_processes() {
        let dir = tempfile::tempdir().unwrap();
        let crashed = Bundle::create(dir.path(), "Crashed").unwrap();
        bundle_with_marker(&crashed, u32::MAX - 1);
        let running = Bundle::create(dir.path(), "Running").unwrap();
        bundle_with_marker(&running, 1);
        let done = Bundle::create(dir.path(), "Done").unwrap();
        fs::remove_file(done.file(MARKER_FILE)).unwrap();

        let names: Vec<String> = list_unfinished(dir.path())
            .unwrap()
            .into_iter()
            .map(|b| b.name)
            .collect();
        assert_eq!(names, vec!["Crashed".to_string()]);
        assert!(
            list_unfinished(&dir.path().join("missing"))
                .unwrap()
                .is_empty()
        );
    }

    fn bundle_with_marker(bundle: &Bundle, pid: u32) {
        fs::write(
            bundle.file(MARKER_FILE),
            serde_json::to_vec(&Marker {
                pid,
                process_started_us: None,
                started_at_unix_ms: 1.0,
                abandoned: false,
            })
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn marker_detects_reused_pid() {
        let me = Marker::for_current_process();
        assert!(me.recorder_alive());
        let reused = Marker {
            process_started_us: me.process_started_us.map(|t| t - 1),
            ..me.clone()
        };
        assert!(!reused.recorder_alive());
        let legacy = Marker {
            process_started_us: None,
            ..me
        };
        assert!(legacy.recorder_alive());
    }

    #[test]
    fn abandoned_bundle_is_listed_while_process_runs() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = Bundle::create(dir.path(), "GaveUp").unwrap();
        assert!(list_unfinished(dir.path()).unwrap().is_empty());
        bundle.abandon().unwrap();
        let listed = list_unfinished(dir.path()).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(Bundle::open_crashed(bundle.path()).is_ok());
    }

    #[test]
    fn lists_unrecoverable_bundles_with_reason() {
        let dir = tempfile::tempdir().unwrap();
        let early = Bundle::create(dir.path(), "Early").unwrap();
        bundle_with_marker(&early, u32::MAX - 1);
        let no_frames = Bundle::create(dir.path(), "NoFrames").unwrap();
        no_frames.save_project(&project("NoFrames", false)).unwrap();
        bundle_with_marker(&no_frames, u32::MAX - 1);

        let listed = list_unfinished(dir.path()).unwrap();
        let problem = |name: &str| {
            listed
                .iter()
                .find(|b| b.name == name)
                .and_then(|b| b.problem.clone())
                .unwrap()
        };
        assert_eq!(problem("Early"), Error::NoProject.to_string());
        assert_eq!(problem("NoFrames"), Error::NoVideo.to_string());
        assert!(Bundle::open_crashed(early.path()).is_ok());
    }

    #[test]
    fn recover_refuses_live_recording() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = Bundle::create(dir.path(), "Live").unwrap();
        bundle.save_project(&project("Live", false)).unwrap();
        bundle_with_marker(&bundle, 1);
        assert!(matches!(
            recover(bundle.path()),
            Err(Error::StillRecording(1))
        ));
    }
}
