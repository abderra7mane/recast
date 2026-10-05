//! Exports a generated bundle and checks the MP4 with AVFoundation.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    thread::sleep,
    time::Duration,
};

use cidre::{av, cm, cv, ns};
use recast_capture::macos::{synthetic, writer::MediaWriter};
use recast_export::{Error, ExportRequest, Progress, export};
use recast_project::{
    AudioTrack, Bundle, CURSORS_DIR, CaptureSource, CursorShape, EVENTS_FILE, EditSettings,
    EventKind, EventLog, FrameRate, InputEvent, MARKER_FILE, MouseButton, Project, Recording, Rect,
    Resolution, SCHEMA_VERSION, SYSTEM_AUDIO_FILE, VIDEO_FILE, VideoTrack,
};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 200;
const SECONDS: i64 = 3;

fn wait(ready: impl Fn() -> bool) {
    while !ready() {
        sleep(Duration::from_millis(1));
    }
}

/// A 3 s, 320×200 recording at 60 fps with no new frames between 1 s and 2 s, a
/// tone on the system audio track, cursor moves and two clicks.
fn fixture(root: &Path) -> PathBuf {
    let bundle = Bundle::create(root, "Fixture").unwrap();
    let base = cm::Time::new(1_000, 1);
    let frame = cm::Time::new(1, 60);

    let mut video = MediaWriter::hevc(&bundle.file(VIDEO_FILE), WIDTH, HEIGHT, 60).unwrap();
    for i in (0..SECONDS * 60).filter(|i| !(60..120).contains(i)) {
        let pts = base.add(cm::Time::new(i, 60));
        let buf = synthetic::video_frame(WIDTH, HEIGHT, i as u64, pts, frame).unwrap();
        wait(|| video.is_ready());
        video.append(&buf).unwrap();
    }
    assert!(
        video
            .finish_at(Some(base.add(cm::Time::new(SECONDS, 1))))
            .unwrap()
    );

    let mut audio = MediaWriter::aac(&bundle.file(SYSTEM_AUDIO_FILE), 2).unwrap();
    for i in 0..(SECONDS as u64 * 48_000 / 1024) {
        let first = i * 1024;
        let pts = base.add(cm::Time::new(first as i64, 48_000));
        let buf = synthetic::audio_chunk(48_000.0, first, 1024, pts).unwrap();
        wait(|| audio.is_ready());
        audio.append(&buf).unwrap();
    }
    assert!(audio.finish().unwrap());

    let (w, h, pixels, hx, hy) = recast_render::cursor::default_arrow_straight(2.0);
    recast_render::bitmap::save_png(&bundle.file("cursors/0.png"), w, h, &pixels).unwrap();
    let mut events = vec![InputEvent {
        t_ms: 0.0,
        kind: EventKind::Cursor { shape: 0 },
    }];
    for i in 0..=150 {
        let t = i as f64 * 20.0;
        events.push(InputEvent {
            t_ms: t,
            kind: EventKind::Move {
                x: 40.0 + t / 3_000.0 * 200.0,
                y: 50.0 + t / 3_000.0 * 100.0,
            },
        });
    }
    for t in [800.0, 1_400.0] {
        let (x, y) = (40.0 + t / 3_000.0 * 200.0, 50.0 + t / 3_000.0 * 100.0);
        events.push(InputEvent {
            t_ms: t,
            kind: EventKind::Down {
                x,
                y,
                button: MouseButton::Left,
                click_count: 1,
            },
        });
        events.push(InputEvent {
            t_ms: t + 90.0,
            kind: EventKind::Up {
                x,
                y,
                button: MouseButton::Left,
            },
        });
    }
    events.sort_by(|a, b| a.t_ms.total_cmp(&b.t_ms));
    bundle
        .save_events(&EventLog {
            cursor_shapes: vec![CursorShape {
                id: 0,
                file: format!("{CURSORS_DIR}/0.png"),
                hash: "arrow".into(),
                hotspot_x: hx,
                hotspot_y: hy,
                width: w as f64 / 2.0,
                height: h as f64 / 2.0,
                scale: 2.0,
            }],
            events,
            ..EventLog::default()
        })
        .unwrap();

    let bounds = Rect {
        x: 0.0,
        y: 0.0,
        width: WIDTH as f64,
        height: HEIGHT as f64,
    };
    bundle
        .save_project(&Project {
            version: SCHEMA_VERSION,
            name: "Fixture".into(),
            created_at_unix_ms: 0.0,
            recording: Recording {
                source: CaptureSource::Display { display_id: 1 },
                bounds,
                width: WIDTH,
                height: HEIGHT,
                scale_factor: 1.0,
                fps: 60,
                duration_ms: SECONDS as f64 * 1000.0,
                video: VideoTrack {
                    file: VIDEO_FILE.into(),
                    codec: "hevc".into(),
                    duration_ms: SECONDS as f64 * 1000.0,
                },
                system_audio: Some(AudioTrack {
                    file: SYSTEM_AUDIO_FILE.into(),
                    offset_ms: 0.0,
                    duration_ms: SECONDS as f64 * 1000.0,
                }),
                mic: None,
                events_file: EVENTS_FILE.into(),
                cursors_dir: CURSORS_DIR.into(),
                recovered: false,
            },
            edits: EditSettings::default(),
        })
        .unwrap();
    fs::remove_file(bundle.file(MARKER_FILE)).unwrap();
    bundle.path().to_path_buf()
}

struct Probe {
    width: f64,
    height: f64,
    fps: f32,
    duration_s: f64,
    video_samples: usize,
    audio_tracks: usize,
}

fn probe(path: &Path) -> Probe {
    let url = ns::Url::with_fs_path_str(&path.to_string_lossy(), false);
    let asset = av::UrlAsset::with_url(&url, None).unwrap();
    let video =
        pollster::block_on(asset.load_tracks_with_media_type(av::MediaType::video())).unwrap();
    let audio =
        pollster::block_on(asset.load_tracks_with_media_type(av::MediaType::audio())).unwrap();
    let track = video.get(0).unwrap();
    let size = track.natural_size();

    let mut reader = av::AssetReader::with_asset(&asset).unwrap();
    let mut output = av::AssetReaderTrackOutput::with_track(&track, None).unwrap();
    reader.add_output(&output).unwrap();
    assert!(reader.start_reading().unwrap());
    let mut video_samples = 0;
    while let Some(sample) = output.next_sample_buf().unwrap() {
        video_samples += sample.num_samples() as usize;
    }
    assert_eq!(reader.status(), av::AssetReaderStatus::Completed);

    Probe {
        width: size.width,
        height: size.height,
        fps: track.nominal_frame_rate(),
        duration_s: asset.duration().as_secs(),
        video_samples,
        audio_tracks: audio.len(),
    }
}

fn settings(fps: FrameRate) -> EditSettings {
    let mut settings = EditSettings::default();
    settings.export.fps = fps;
    settings.export.resolution = Resolution::P1080;
    settings
}

#[test]
fn exports_mp4_with_expected_size_rate_duration_and_audio() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = fixture(dir.path());
    let output = dir.path().join("out.mp4");
    let mut progress = Vec::new();
    let summary = export(
        &ExportRequest {
            bundle: &bundle,
            output: &output,
            settings: Some(settings(FrameRate::Fps30)),
        },
        &mut |p: Progress| progress.push(p),
        &AtomicBool::new(false),
    )
    .unwrap();

    let (w, h) = recast_render::layout::output_size(320.0, 200.0, 0.08, Resolution::P1080);
    assert_eq!((summary.width, summary.height), (w, h));
    assert_eq!(h, 1080);
    assert_eq!((summary.fps, summary.frames), (30, 90));
    assert!(summary.audio);
    assert_eq!(progress.len(), 90);
    assert_eq!(
        progress.last(),
        Some(&Progress {
            frame: 90,
            total_frames: 90
        })
    );

    let probe = probe(&output);
    assert_eq!((probe.width, probe.height), (w as f64, h as f64));
    assert!((probe.fps - 30.0).abs() < 0.1, "{}", probe.fps);
    assert!(
        (probe.duration_s - 3.0).abs() < 0.05,
        "{}",
        probe.duration_s
    );
    assert_eq!(probe.video_samples, 90);
    assert_eq!(probe.audio_tracks, 1);
}

#[test]
fn trim_shortens_the_export() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = fixture(dir.path());
    let output = dir.path().join("trimmed.mp4");
    let mut settings = settings(FrameRate::Fps60);
    settings.trim.start_ms = 500.0;
    settings.trim.end_ms = Some(2_000.0);
    settings.sounds.enabled = false;
    settings.audio.system_volume = 0.0;
    let summary = export(
        &ExportRequest {
            bundle: &bundle,
            output: &output,
            settings: Some(settings),
        },
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!((summary.fps, summary.frames), (60, 90));
    assert!(!summary.audio);
    let probe = probe(&output);
    assert!(
        (probe.duration_s - 1.5).abs() < 0.05,
        "{}",
        probe.duration_s
    );
    assert_eq!(probe.video_samples, 90);
    assert_eq!(probe.audio_tracks, 0);
}

#[test]
fn cancelling_removes_the_partial_file() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = fixture(dir.path());
    let output = dir.path().join("cancelled.mp4");
    let cancel = AtomicBool::new(false);
    let result = export(
        &ExportRequest {
            bundle: &bundle,
            output: &output,
            settings: None,
        },
        &mut |p: Progress| {
            if p.frame == 10 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    );
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert!(!output.exists());
}

fn read_output(
    path: &Path,
    media: &av::MediaType,
    settings: &ns::Dictionary<ns::String, ns::Id>,
) -> Vec<cidre::arc::R<cm::SampleBuf>> {
    let url = ns::Url::with_fs_path_str(&path.to_string_lossy(), false);
    let asset = av::UrlAsset::with_url(&url, None).unwrap();
    let tracks = pollster::block_on(asset.load_tracks_with_media_type(media)).unwrap();
    let track = tracks.get(0).unwrap();
    let mut reader = av::AssetReader::with_asset(&asset).unwrap();
    let mut output = av::AssetReaderTrackOutput::with_track(&track, Some(settings)).unwrap();
    reader.add_output(&output).unwrap();
    assert!(reader.start_reading().unwrap());
    let mut samples = Vec::new();
    while let Some(sample) = output.next_sample_buf().unwrap() {
        samples.push(sample);
    }
    assert_eq!(reader.status(), av::AssetReaderStatus::Completed);
    samples
}

/// The output's audio as interleaved stereo float at 48 kHz.
fn audio(path: &Path) -> Vec<f32> {
    use av::audio::{all_formats_keys as all, linear_pcm_keys as pcm};
    let mut settings = ns::DictionaryMut::<ns::String, ns::Id>::with_capacity(6);
    settings.insert(
        all::id(),
        ns::Number::with_u32(u32::from_be_bytes(*b"lpcm")).as_id_ref(),
    );
    settings.insert(
        all::sample_rate(),
        ns::Number::with_f64(48_000.0).as_id_ref(),
    );
    settings.insert(
        all::number_of_channels(),
        ns::Number::with_u32(2).as_id_ref(),
    );
    settings.insert(pcm::bit_depth(), ns::Number::with_u32(32).as_id_ref());
    settings.insert(pcm::is_float(), ns::Number::with_bool(true).as_id_ref());
    settings.insert(
        pcm::is_non_interleaved(),
        ns::Number::with_bool(false).as_id_ref(),
    );
    let mut samples = Vec::new();
    for buf in read_output(path, av::MediaType::audio(), &settings) {
        let block = buf.data_buf().unwrap();
        let mut bytes = vec![0u8; block.data_len()];
        block.copy_to(0, &mut bytes).unwrap();
        samples.extend(
            bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        );
    }
    samples
}

/// Decoded video frames at `indices`, as tightly packed BGRA.
fn frames(path: &Path, indices: &[usize]) -> Vec<Vec<u8>> {
    let mut settings = ns::DictionaryMut::<ns::String, ns::Id>::with_capacity(1);
    settings.insert(
        cv::pixel_buffer_keys::pixel_format().as_ns(),
        ns::Number::with_u32(cv::PixelFormat::_32_BGRA.0).as_id_ref(),
    );
    let samples = read_output(path, av::MediaType::video(), &settings);
    indices
        .iter()
        .map(|&i| {
            let mut pixels = samples[i].image_buf().unwrap().retained();
            let (w, h, stride) = (pixels.width(), pixels.height(), pixels.bytes_per_row());
            // SAFETY: locked read-only while copied, unlocked right after.
            unsafe {
                pixels
                    .lock_base_addr(cv::pixel_buffer::LockFlags::READ_ONLY)
                    .result()
                    .unwrap();
                let data =
                    std::slice::from_raw_parts(pixels.base_address() as *const u8, stride * h);
                let packed = (0..h)
                    .flat_map(|y| data[y * stride..y * stride + w * 4].to_vec())
                    .collect();
                let _ = pixels.unlock_lock_base_addr(cv::pixel_buffer::LockFlags::READ_ONLY);
                packed
            }
        })
        .collect()
}

fn mean_difference(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(*y) as u64)
        .sum::<u64>() as f64
        / a.len() as f64
}

/// Times (seconds) where sound starts after at least 100 ms of silence.
fn onsets(samples: &[f32]) -> Vec<f64> {
    let mut found = Vec::new();
    let mut quiet = usize::MAX;
    for (frame, pair) in samples.chunks_exact(2).enumerate() {
        if pair[0].abs().max(pair[1].abs()) > 0.01 {
            if quiet >= 4_800 {
                found.push(frame as f64 / 48_000.0);
            }
            quiet = 0;
        } else {
            quiet = quiet.saturating_add(1);
        }
    }
    found
}

#[test]
fn click_sounds_and_trimmed_frames_line_up_with_the_recording() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = fixture(dir.path());
    let run = |name: &str, trim_start: f64| {
        let output = dir.path().join(name);
        let mut settings = settings(FrameRate::Fps30);
        settings.audio.system_volume = 0.0;
        settings.trim.start_ms = trim_start;
        export(
            &ExportRequest {
                bundle: &bundle,
                output: &output,
                settings: Some(settings),
            },
            &mut |_| {},
            &AtomicBool::new(false),
        )
        .unwrap();
        output
    };
    let full = run("full.mp4", 0.0);
    let trimmed = run("trimmed.mp4", 500.0);

    let presses = onsets(&audio(&full));
    assert_eq!(presses.len(), 2, "{presses:?}");
    assert!((presses[0] - 0.8).abs() < 0.005, "{presses:?}");
    assert!((presses[1] - 1.4).abs() < 0.005, "{presses:?}");
    let shifted = onsets(&audio(&trimmed));
    assert!((shifted[0] - 0.3).abs() < 0.005, "{shifted:?}");

    let full_frames = frames(&full, &[0, 15]);
    let first_trimmed = &frames(&trimmed, &[0])[0];
    let same = mean_difference(first_trimmed, &full_frames[1]);
    let other = mean_difference(first_trimmed, &full_frames[0]);
    assert!(same < 1.5, "trimmed frame 0 vs untrimmed 0.5 s: {same}");
    assert!(
        other > same * 4.0,
        "frame 0 differs from 0.5 s only by {other}"
    );
}
