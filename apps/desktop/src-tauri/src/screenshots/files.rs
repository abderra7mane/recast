//! Where screenshots are written and what they are called.

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Local};
use serde::Serialize;
use specta::Type;

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

/// A saved screenshot, for the Library and Recent Screenshots.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotSummary {
    pub path: String,
    /// File name without the extension.
    pub name: String,
    pub modified_at_unix_ms: f64,
    pub width: u32,
    pub height: u32,
}

/// The PNG files in `dir`, last modified first.
pub fn list_screenshots(dir: &Path) -> Vec<ScreenshotSummary> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut shots: Vec<ScreenshotSummary> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path
                .extension()
                .is_none_or(|e| !e.eq_ignore_ascii_case("png"))
            {
                return None;
            }
            let (width, height) = png_size(&path)?;
            let modified = entry.metadata().and_then(|m| m.modified()).ok()?;
            Some(ScreenshotSummary {
                name: path.file_stem()?.to_string_lossy().into_owned(),
                path: path.to_string_lossy().into_owned(),
                modified_at_unix_ms: modified
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64()
                    * 1000.0,
                width,
                height,
            })
        })
        .collect();
    shots.sort_by(|a, b| b.modified_at_unix_ms.total_cmp(&a.modified_at_unix_ms));
    shots
}

/// Width and height from a PNG's header, or `None` when it isn't a PNG.
fn png_size(path: &Path) -> Option<(u32, u32)> {
    use std::io::Read;

    let mut header = [0u8; 24];
    std::fs::File::open(path)
        .ok()?
        .read_exact(&mut header)
        .ok()?;
    if &header[..8] != b"\x89PNG\r\n\x1a\n" || &header[12..16] != b"IHDR" {
        return None;
    }
    let number = |at: usize| u32::from_be_bytes(header[at..at + 4].try_into().unwrap());
    Some((number(16), number(20)))
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
    fn png(width: u32, height: u32) -> Vec<u8> {
        recast_render::bitmap::encode_png(width, height, &vec![0; (width * height * 4) as usize])
            .unwrap()
    }

    #[test]
    fn lists_screenshots_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("old.png"), png(4, 3)).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(dir.path().join("new.PNG"), png(2, 5)).unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();
        std::fs::write(dir.path().join("broken.png"), b"not a png").unwrap();

        let shots = list_screenshots(dir.path());
        let names: Vec<&str> = shots.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["new", "old"]);
        assert_eq!((shots[0].width, shots[0].height), (2, 5));
        assert_eq!((shots[1].width, shots[1].height), (4, 3));
        assert!(shots[0].path.ends_with("new.PNG"));
    }

    #[test]
    fn a_missing_folder_has_no_screenshots() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_screenshots(&dir.path().join("missing")).is_empty());
    }
}
