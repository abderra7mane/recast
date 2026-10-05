//! Command-line access to recording, for testing without the UI.

use std::{path::PathBuf, process::exit, thread::sleep, time::Duration};

use recast_capture::{CaptureTarget, Rect, ScreenCapture};
use recast_desktop_lib::recording::{RecordingRequest, Session, default_name};

const USAGE: &str = "usage:
  recast-cli permissions
  recast-cli displays | windows
  recast-cli record --seconds N (--display ID | --window ID | --region ID,X,Y,W,H)
                    [--system-audio] [--mic] [--root DIR] [--name NAME]
  recast-cli simulate --seconds N [--system-audio] [--root DIR] [--name NAME]
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
        target: None,
        system_audio: false,
        mic: false,
        root: recast_project::default_root(),
        name: default_name(),
    };
    while let Some(flag) = rest.next() {
        match flag.as_str() {
            "--seconds" => args.seconds = number(rest.next(), "--seconds"),
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

fn record(args: Args, capture: &dyn ScreenCapture) {
    let target = args
        .target
        .unwrap_or(CaptureTarget::Display { display_id: 0 });
    let request = RecordingRequest {
        target,
        system_audio: args.system_audio,
        mic: args.mic,
    };
    let session = Session::start(
        &args.root,
        &args.name,
        &request,
        capture,
        &recast_input::platform(),
    )
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
            record(args, &recast_capture::platform());
        }
        #[cfg(feature = "synthetic")]
        "simulate" => record(
            parse(argv),
            &recast_desktop_lib::synthetic_capture::SyntheticCapture {
                width: 640,
                height: 360,
            },
        ),
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
