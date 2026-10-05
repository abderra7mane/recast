//! Command-line access to recording and export, for testing without the UI.

use std::{
    path::{Path, PathBuf},
    process::exit,
    sync::atomic::AtomicBool,
    thread::sleep,
    time::{Duration, Instant},
};

use recast_capture::{CaptureTarget, Rect, ScreenCapture};
use recast_desktop_lib::recording::{RecordingRequest, Session, default_name};
use recast_input::InputCapture;
use recast_project::{Bundle, EditSettings};
use serde_json::Value;

const USAGE: &str = "usage:
  recast-cli permissions
  recast-cli displays | windows
  recast-cli record --seconds N (--display ID | --window ID | --region ID,X,Y,W,H)
                    [--system-audio] [--mic] [--root DIR] [--name NAME]
  recast-cli simulate --seconds N [--size WxH] [--system-audio] [--root DIR] [--name NAME]
  recast-cli export BUNDLE [--out FILE.mp4] [--settings FILE.json] [--set PATH=VALUE]...
                    e.g. --set export.fps=30 --set zoom.level=2.5 --set trim.endMs=4000
  recast-cli unfinished [--root DIR]
  recast-cli recover BUNDLE
  recast-cli discard BUNDLE";

/// Prints `log` records to stderr when `RECAST_LOG=debug` is set.
struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        eprintln!("[{}] {}", record.level(), record.args());
    }
    fn flush(&self) {}
}

static LOGGER: StderrLogger = StderrLogger;

struct Args {
    seconds: f64,
    size: (u32, u32),
    target: Option<CaptureTarget>,
    system_audio: bool,
    mic: bool,
    root: PathBuf,
    name: String,
}

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("{message}");
    exit(1)
}

fn number<T: std::str::FromStr>(value: Option<String>, flag: &str) -> T {
    value
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| fail(format!("{flag} needs a number\n\n{USAGE}")))
}

fn parse(mut rest: impl Iterator<Item = String>) -> Args {
    let mut args = Args {
        seconds: 5.0,
        size: (640, 360),
        target: None,
        system_audio: false,
        mic: false,
        root: recast_project::default_root(),
        name: default_name(),
    };
    while let Some(flag) = rest.next() {
        match flag.as_str() {
            "--seconds" => args.seconds = number(rest.next(), "--seconds"),
            "--size" => {
                let raw = rest.next().unwrap_or_default();
                args.size = raw
                    .split_once('x')
                    .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
                    .filter(|&(w, h): &(u32, u32)| w >= 16 && h >= 16 && w % 2 == 0 && h % 2 == 0)
                    .unwrap_or_else(|| {
                        fail(format!("--size needs even WxH, e.g. 1280x720\n\n{USAGE}"))
                    });
            }
            "--display" => {
                args.target = Some(CaptureTarget::Display {
                    display_id: number(rest.next(), "--display"),
                })
            }
            "--window" => {
                args.target = Some(CaptureTarget::Window {
                    window_id: number(rest.next(), "--window"),
                })
            }
            "--region" => {
                let raw = rest.next().unwrap_or_default();
                let parts: Vec<f64> = raw.split(',').filter_map(|p| p.parse().ok()).collect();
                let [id, x, y, width, height] = parts[..] else {
                    fail(format!("--region needs ID,X,Y,W,H\n\n{USAGE}"));
                };
                args.target = Some(CaptureTarget::Region {
                    display_id: id as u32,
                    rect: Rect {
                        x,
                        y,
                        width,
                        height,
                    },
                });
            }
            "--system-audio" => args.system_audio = true,
            "--mic" => args.mic = true,
            "--root" => args.root = rest.next().map(PathBuf::from).unwrap_or(args.root),
            "--name" => args.name = rest.next().unwrap_or(args.name),
            other => fail(format!("unknown option {other}\n\n{USAGE}")),
        }
    }
    args
}

fn print_json<T: serde::Serialize>(value: &T) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_else(|e| fail(e))
    );
}

fn record(args: Args, capture: &dyn ScreenCapture, input: &dyn InputCapture) {
    let target = args
        .target
        .unwrap_or(CaptureTarget::Display { display_id: 0 });
    let request = RecordingRequest {
        target,
        system_audio: args.system_audio,
        mic: args.mic,
    };
    let session = Session::start(&args.root, &args.name, &request, capture, input)
        .unwrap_or_else(|e| fail(format!("cannot start: {e}")));
    let status = session.status();
    eprintln!(
        "recording {}x{} into {} (input events: {})",
        status.width, status.height, status.bundle_path, status.input_events
    );
    sleep(Duration::from_secs_f64(args.seconds));
    let finished = session
        .stop()
        .unwrap_or_else(|e| fail(format!("cannot stop: {e}")));
    for warning in &finished.warnings {
        eprintln!("warning: {warning}");
    }
    print_json(&finished);
}

/// Sets the field at a dotted `path` (e.g. `zoom.level`). The value is read as JSON
/// unless the field holds a string, so `export.fps=30` stays the string "30".
fn set_path(root: &mut Value, path: &str, raw: &str) {
    let mut target = root;
    for key in path.split('.') {
        if !target.is_object() {
            *target = Value::Object(Default::default());
        }
        target = target
            .as_object_mut()
            .expect("object")
            .entry(key)
            .or_insert(Value::Null);
    }
    *target = match (&*target, serde_json::from_str(raw)) {
        (Value::String(_), _) | (_, Err(_)) => Value::String(raw.into()),
        (_, Ok(value)) => value,
    };
}

fn merge(base: &mut Value, overrides: Value) {
    match (base, overrides) {
        (Value::Object(base), Value::Object(overrides)) => {
            for (key, value) in overrides {
                merge(base.entry(key).or_insert(Value::Null), value);
            }
        }
        (base, value) => *base = value,
    }
}

fn export_settings(
    saved: &EditSettings,
    mut rest: impl Iterator<Item = String>,
) -> (EditSettings, Option<PathBuf>) {
    let mut settings = serde_json::to_value(saved).unwrap_or_else(|e| fail(e));
    let mut out = None;
    while let Some(flag) = rest.next() {
        let value = rest
            .next()
            .unwrap_or_else(|| fail(format!("{flag} needs a value\n\n{USAGE}")));
        match flag.as_str() {
            "--out" => out = Some(PathBuf::from(value)),
            "--settings" => {
                let text = std::fs::read_to_string(&value)
                    .unwrap_or_else(|e| fail(format!("{value}: {e}")));
                let overrides: Value =
                    serde_json::from_str(&text).unwrap_or_else(|e| fail(format!("{value}: {e}")));
                merge(&mut settings, overrides);
            }
            "--set" => {
                let (path, raw) = value
                    .split_once('=')
                    .unwrap_or_else(|| fail(format!("--set needs PATH=VALUE\n\n{USAGE}")));
                set_path(&mut settings, path, raw);
            }
            other => fail(format!("unknown option {other}\n\n{USAGE}")),
        }
    }
    let settings =
        serde_json::from_value(settings).unwrap_or_else(|e| fail(format!("invalid settings: {e}")));
    (settings, out)
}

fn default_output(bundle: &Path) -> PathBuf {
    let name = bundle
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "export".into());
    bundle.with_file_name(format!("{name}.mp4"))
}

fn export(mut argv: impl Iterator<Item = String>) {
    let path = PathBuf::from(argv.next().unwrap_or_else(|| fail(USAGE)));
    let bundle = Bundle::open(&path).unwrap_or_else(|e| fail(e));
    let project = bundle.load_project().unwrap_or_else(|e| fail(e));
    let (settings, out) = export_settings(&project.edits, argv);
    let output = out.unwrap_or_else(|| default_output(&path));

    let mut last_report = Instant::now();
    let summary = recast_export::export(
        &recast_export::ExportRequest {
            bundle: &path,
            output: &output,
            settings: Some(settings),
        },
        &mut |p| {
            if last_report.elapsed() >= Duration::from_secs(1) || p.frame == p.total_frames {
                eprintln!("frame {}/{}", p.frame, p.total_frames);
                last_report = Instant::now();
            }
        },
        &AtomicBool::new(false),
    )
    .unwrap_or_else(|e| fail(format!("export failed: {e}")));
    let seconds = summary.elapsed.as_secs_f64();
    print_json(&serde_json::json!({
        "output": output.display().to_string(),
        "width": summary.width,
        "height": summary.height,
        "fps": summary.fps,
        "frames": summary.frames,
        "durationMs": summary.duration_ms,
        "audio": summary.audio,
        "elapsedSeconds": seconds,
        "framesPerSecond": summary.frames as f64 / seconds.max(1e-9),
    }));
}

fn main() {
    if std::env::var("RECAST_LOG").as_deref() == Ok("debug") {
        let _ = log::set_logger(&LOGGER).map(|()| log::set_max_level(log::LevelFilter::Debug));
    }
    let mut argv = std::env::args().skip(1);
    let command = argv.next().unwrap_or_default();
    match command.as_str() {
        "permissions" => print_json(&recast_input::check_permissions()),
        "displays" => print_json(
            &recast_capture::platform()
                .displays()
                .unwrap_or_else(|e| fail(e)),
        ),
        "windows" => print_json(
            &recast_capture::platform()
                .windows()
                .unwrap_or_else(|e| fail(e)),
        ),
        "record" => {
            let mut args = parse(argv);
            if args.target.is_none() {
                let displays = recast_capture::platform()
                    .displays()
                    .unwrap_or_else(|e| fail(e));
                let main = displays.first().unwrap_or_else(|| fail("no display found"));
                args.target = Some(CaptureTarget::Display {
                    display_id: main.id,
                });
            }
            record(args, &recast_capture::platform(), &recast_input::platform());
        }
        #[cfg(feature = "synthetic")]
        "simulate" => {
            let args = parse(argv);
            let (width, height) = args.size;
            record(
                args,
                &recast_desktop_lib::synthetic_capture::SyntheticCapture { width, height },
                &recast_desktop_lib::synthetic_input::SyntheticInput {
                    width: width as f64,
                    height: height as f64,
                },
            )
        }
        "export" => export(argv),
        "unfinished" => {
            let args = parse(argv);
            print_json(&recast_project::list_unfinished(&args.root).unwrap_or_else(|e| fail(e)));
        }
        "recover" => {
            let path = argv.next().unwrap_or_else(|| fail(USAGE));
            print_json(&recast_project::recover(&PathBuf::from(path)).unwrap_or_else(|e| fail(e)));
        }
        "discard" => {
            let path = argv.next().unwrap_or_else(|| fail(USAGE));
            recast_desktop_lib::discard_bundle(&PathBuf::from(&path)).unwrap_or_else(|e| fail(e));
            eprintln!("moved {path} to the Trash");
        }
        _ => fail(USAGE),
    }
}
