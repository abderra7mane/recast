//! Puts crashes in the log: Rust panics and uncaught Objective-C exceptions as they
//! happen, and the crash reports macOS wrote for the previous run at the next start.

use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use objc2_foundation::{NSException, NSSetUncaughtExceptionHandler};

/// Crash report names start with the executable's name.
const REPORT_PREFIXES: &[&str] = &["recast-desktop-", "Recast-"];

pub fn install_handlers() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let backtrace = std::backtrace::Backtrace::force_capture();
        log::error!(
            "panic on thread {}: {info}\n{backtrace}",
            thread.name().unwrap_or("unnamed")
        );
        log::logger().flush();
        previous(info);
    }));

    extern "C" fn on_exception(exception: &NSException) {
        let reason = exception
            .reason()
            .map(|r| r.to_string())
            .unwrap_or_default();
        let stack: Vec<String> = exception
            .callStackSymbols()
            .iter()
            .map(|s| s.to_string())
            .collect();
        log::error!(
            "uncaught exception {}: {reason}\n{}",
            exception.name(),
            stack.join("\n")
        );
        log::logger().flush();
    }
    let handler: extern "C" fn(&NSException) = on_exception;
    // SAFETY: the handler has the `void (*)(NSException *)` signature Foundation calls.
    unsafe { NSSetUncaughtExceptionHandler(handler as *mut _) };
}

pub fn reports_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("Library/Logs/DiagnosticReports")
}

/// Recast's crash reports in `dir` written after `since`, oldest first.
pub fn reports_since(dir: &Path, since: SystemTime) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut reports: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| {
            e.file_name().to_str().is_some_and(|name| {
                name.ends_with(".ips") && REPORT_PREFIXES.iter().any(|p| name.starts_with(p))
            })
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .filter(|(modified, _)| *modified > since)
        .collect();
    reports.sort();
    reports.into_iter().map(|(_, path)| path).collect()
}

/// When the log was last written, which is about when the previous run ended. Read it
/// before logging starts.
pub fn last_log_write(log_file: &Path) -> Option<SystemTime> {
    std::fs::metadata(log_file).and_then(|m| m.modified()).ok()
}

/// Logs the crash reports of runs that ended after `since`.
pub fn log_reports_since(since: Option<SystemTime>) {
    let Some(since) = since else {
        return;
    };
    for report in reports_since(&reports_dir(), since) {
        log::warn!("Recast crashed earlier; crash report: {}", report.display());
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn finds_only_new_recast_reports() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str| std::fs::write(dir.path().join(name), b"{}").unwrap();
        write("recast-desktop-2026-10-06-034133.ips");
        let since = SystemTime::now() - Duration::from_secs(60);
        write("Recast-2026-10-06-040000.ips");
        write("Safari-2026-10-06-040000.ips");
        write("recast-desktop-notes.txt");

        let names: Vec<String> = reports_since(dir.path(), since)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"recast-desktop-2026-10-06-034133.ips".to_string()));
        assert!(names.contains(&"Recast-2026-10-06-040000.ips".to_string()));

        assert!(reports_since(dir.path(), SystemTime::now() + Duration::from_secs(60)).is_empty());
        assert!(reports_since(&dir.path().join("missing"), since).is_empty());
    }
}
