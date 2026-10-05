//! `fixtures/crashed.recast` is a real bundle whose recorder was killed with SIGKILL
//! about five seconds in: AVAssetWriter output with a partial trailing fragment.

use std::{fs, path::Path};

use recast_project::{
    Bundle, EVENTS_FILE, EventKind, MARKER_FILE, RECORD_LOG_FILE, list_unfinished,
    mp4::{self, TrackKind},
    recover,
};

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn crashed_copy(root: &Path) -> std::path::PathBuf {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/crashed.recast");
    let bundle = root.join("crashed.recast");
    copy_dir(&fixture, &bundle);
    fs::write(
        bundle.join(MARKER_FILE),
        r#"{"pid":4294967294,"startedAtUnixMs":1.0}"#,
    )
    .unwrap();
    bundle
}

#[test]
fn recovers_truncated_bundle() {
    let root = tempfile::tempdir().unwrap();
    let path = crashed_copy(root.path());

    let before = mp4::scan_file(&path.join("screen.mp4")).unwrap();
    assert!(
        before.complete_len < before.file_len,
        "fixture has a partial tail"
    );

    let listed = list_unfinished(root.path()).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "crashed");
    assert_eq!(listed[0].problem, None);

    let project = recover(&path).unwrap();
    let recording = &project.recording;
    assert!(recording.recovered);
    assert_eq!(recording.video.duration_ms, 4000.0);
    assert_eq!(recording.duration_ms, 4000.0);
    let audio = recording.system_audio.as_ref().unwrap();
    assert!((3900.0..4100.0).contains(&audio.duration_ms));
    assert!(recording.mic.is_none());

    let after = mp4::scan_file(&path.join("screen.mp4")).unwrap();
    assert_eq!(after.complete_len, after.file_len);
    assert_eq!(after.complete_len, before.complete_len);
    assert!(after.track(TrackKind::Video).is_some());

    assert!(!path.join(MARKER_FILE).exists());
    assert!(!path.join(RECORD_LOG_FILE).exists());
    assert!(path.join(EVENTS_FILE).exists());
    assert!(list_unfinished(root.path()).unwrap().is_empty());

    let bundle = Bundle::open(&path).unwrap();
    assert_eq!(bundle.load_project().unwrap(), project);
    let events = bundle.load_events().unwrap();
    assert_eq!(events.cursor_shapes.len(), 1);
    assert!(fs::metadata(path.join(&events.cursor_shapes[0].file)).is_ok());
    assert!(matches!(
        events.events.first().map(|e| &e.kind),
        Some(EventKind::Cursor { shape: 0 })
    ));
}
