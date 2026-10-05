//! Where screenshots are written and what they are called.

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Local};

/// `~/Pictures/Recast`
pub fn screenshots_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("Pictures")
        .join("Recast")
}

/// "Recast 2026-10-05 at 14.03.22", the name of a capture taken at `time`.
pub fn capture_name(time: DateTime<Local>) -> String {
    time.format("Recast %Y-%m-%d at %H.%M.%S").to_string()
}

/// `dir/<stem>.png`, or `dir/<stem> (2).png` and so on when that name is taken.
pub fn unique_path(dir: &Path, stem: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.png"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}).png")))
        .find(|path| !path.exists())
        .expect("a free name")
}

/// Writes `png` under a free name in `dir`, creating `dir` when needed. The file is
/// created exclusively, so a concurrent save never overwrites it.
pub fn save_png(dir: &Path, stem: &str, png: &[u8]) -> std::io::Result<PathBuf> {
    use std::io::Write;

    std::fs::create_dir_all(dir)?;
    loop {
        let path = unique_path(dir, stem);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(png)?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

/// Removes PNG files in `dir` last modified before `max_age` ago.
pub fn prune(dir: &Path, max_age: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|modified| now.duration_since(modified).unwrap_or_default() > max_age);
        if old && path.extension().is_some_and(|e| e == "png") {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn names_follow_the_date_and_time() {
        let time = Local.with_ymd_and_hms(2026, 10, 5, 9, 3, 7).unwrap();
        assert_eq!(capture_name(time), "Recast 2026-10-05 at 09.03.07");
    }

    #[test]
    fn taken_names_get_a_number() {
        let dir = tempfile::tempdir().unwrap();
        let stem = "Recast 2026-10-05 at 09.03.07";
        let first = save_png(dir.path(), stem, b"one").unwrap();
        let second = save_png(dir.path(), stem, b"two").unwrap();
        let third = save_png(dir.path(), stem, b"three").unwrap();
        assert_eq!(
            first.file_name().unwrap(),
            "Recast 2026-10-05 at 09.03.07.png"
        );
        assert_eq!(
            second.file_name().unwrap(),
            "Recast 2026-10-05 at 09.03.07 (2).png"
        );
        assert_eq!(
            third.file_name().unwrap(),
            "Recast 2026-10-05 at 09.03.07 (3).png"
        );
        assert_eq!(std::fs::read(first).unwrap(), b"one");
        assert_eq!(std::fs::read(third).unwrap(), b"three");

        std::fs::remove_file(&second).unwrap();
        assert_eq!(unique_path(dir.path(), stem), second);
    }

    #[test]
    fn save_creates_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("Pictures").join("Recast");
        let path = save_png(&nested, "shot", b"png").unwrap();
        assert_eq!(path, nested.join("shot.png"));
    }

    #[test]
    fn prune_keeps_recent_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.png"), b"x").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();
        prune(dir.path(), Duration::from_secs(3600));
        assert!(dir.path().join("a.png").exists());
        std::thread::sleep(Duration::from_millis(10));
        prune(dir.path(), Duration::ZERO);
        assert!(!dir.path().join("a.png").exists());
        assert!(dir.path().join("notes.txt").exists());
    }
}
