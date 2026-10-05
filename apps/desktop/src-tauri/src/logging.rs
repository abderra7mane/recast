//! Rotating log files in `~/Library/Logs/Recast`, written from Rust and the webviews.

use std::path::{Path, PathBuf};

use log::LevelFilter;
use tauri::{Runtime, plugin::TauriPlugin};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

pub struct LogConfig {
    pub dir: PathBuf,
    /// The active file is `<file_name>.log`; rotated ones get a date suffix.
    pub file_name: &'static str,
    pub max_file_bytes: u128,
    /// Files kept, the active one included.
    pub keep_files: usize,
    pub level: LevelFilter,
    /// Chatty dependencies only log warnings and errors.
    pub quiet: &'static [&'static str],
}

pub fn config() -> LogConfig {
    LogConfig {
        dir: std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("Library")
            .join("Logs")
            .join("Recast"),
        file_name: "Recast",
        max_file_bytes: 5 * 1024 * 1024,
        keep_files: 5,
        level: LevelFilter::Info,
        quiet: &[
            "wgpu_core",
            "wgpu_hal",
            "naga",
            "tao",
            "wry",
            "reqwest",
            "hyper",
        ],
    }
}

impl LogConfig {
    pub fn active_file(&self) -> PathBuf {
        self.dir.join(format!("{}.log", self.file_name))
    }

    pub fn plugin<R: Runtime>(&self) -> TauriPlugin<R> {
        let mut builder = tauri_plugin_log::Builder::new()
            .clear_targets()
            .target(Target::new(TargetKind::Folder {
                path: self.dir.clone(),
                file_name: Some(self.file_name.into()),
            }))
            .max_file_size(self.max_file_bytes)
            .rotation_strategy(RotationStrategy::KeepSome(self.keep_files))
            .timezone_strategy(TimezoneStrategy::UseLocal)
            .level(self.level);
        if cfg!(debug_assertions) {
            builder = builder.target(Target::new(TargetKind::Stdout));
        }
        for module in self.quiet {
            builder = builder.level_for(*module, LevelFilter::Warn);
        }
        builder.build()
    }

    /// The last `count` lines, reaching into rotated files when the active one is short.
    pub fn tail(&self, count: usize) -> Vec<String> {
        tail(&self.dir, self.file_name, count)
    }
}

/// Log files of `file_name` in `dir`, oldest first; rotated names sort by date.
fn files(dir: &Path, file_name: &str) -> Vec<PathBuf> {
    let active = format!("{file_name}.log");
    let rotated_prefix = format!("{file_name}_");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut rotated: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.starts_with(&rotated_prefix) && n.ends_with(".log") && n != active
            })
        })
        .collect();
    rotated.sort();
    let active = dir.join(active);
    if active.is_file() {
        rotated.push(active);
    }
    rotated
}

fn tail(dir: &Path, file_name: &str, count: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for file in files(dir, file_name).iter().rev() {
        if lines.len() >= count {
            break;
        }
        let Ok(bytes) = std::fs::read(file) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let mut chunk: Vec<String> = text
            .lines()
            .rev()
            .take(count - lines.len())
            .map(str::to_owned)
            .collect();
        chunk.reverse();
        chunk.append(&mut lines);
        lines = chunk;
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_rotate_in_the_recast_logs_folder() {
        let config = config();
        assert!(config.dir.ends_with("Library/Logs/Recast"));
        assert_eq!(config.active_file(), config.dir.join("Recast.log"));
        assert_eq!(config.max_file_bytes, 5 * 1024 * 1024);
        assert_eq!(config.keep_files, 5);
        assert_eq!(config.level, LevelFilter::Info);
        assert!(config.quiet.contains(&"wgpu_core"));
    }

    #[test]
    fn tail_reads_back_into_rotated_files() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, lines: &[&str]| {
            std::fs::write(dir.path().join(name), lines.join("\n") + "\n").unwrap();
        };
        write("Recast_2026-10-01_10-00-00.log", &["a1", "a2"]);
        write("Recast_2026-10-03_10-00-00.log", &["b1", "b2", "b3"]);
        write("Recast.log", &["c1", "c2"]);
        write("Other.log", &["x"]);

        assert_eq!(tail(dir.path(), "Recast", 1), ["c2"]);
        assert_eq!(tail(dir.path(), "Recast", 4), ["b2", "b3", "c1", "c2"]);
        assert_eq!(
            tail(dir.path(), "Recast", 50),
            ["a1", "a2", "b1", "b2", "b3", "c1", "c2"]
        );
        assert!(tail(&dir.path().join("missing"), "Recast", 5).is_empty());
    }
}
