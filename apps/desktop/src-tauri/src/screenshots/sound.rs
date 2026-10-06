//! The shutter sound: macOS's own screenshot sound, played from the system's copy
//! through System Sound Services, the API for short user-interface sounds.

use std::{
    ffi::c_void,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use objc2_foundation::{NSString, NSURL};

const SYSTEM_SOUNDS: &str =
    "/System/Library/Components/CoreAudio.component/Contents/SharedSupport/SystemSounds/system";
/// The current name first, then the one older macOS versions used.
const NAMES: &[&str] = &["Screen Capture.aif", "Grab.aif"];

type SystemSoundId = u32;

#[link(name = "AudioToolbox", kind = "framework")]
unsafe extern "C" {
    fn AudioServicesCreateSystemSoundID(url: *const c_void, sound: *mut SystemSoundId) -> i32;
    fn AudioServicesPlaySystemSound(sound: SystemSoundId);
}

/// The first of `names` that exists in `dir`.
pub fn find(dir: &Path, names: &[&str]) -> Option<PathBuf> {
    names.iter().map(|n| dir.join(n)).find(|p| p.is_file())
}

fn load(path: &Path) -> Option<SystemSoundId> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let mut sound = 0;
    // SAFETY: NSURL is toll-free bridged with CFURL, and `sound` is a valid out pointer.
    let status =
        unsafe { AudioServicesCreateSystemSoundID((&*url as *const NSURL).cast(), &mut sound) };
    if status == 0 {
        Some(sound)
    } else {
        log::warn!("cannot load {}: OSStatus {status}", path.display());
        None
    }
}

/// Plays the screenshot sound; does nothing when this macOS doesn't have it.
pub fn play_shutter() {
    static SOUND: OnceLock<Option<SystemSoundId>> = OnceLock::new();
    let sound = SOUND.get_or_init(|| match find(Path::new(SYSTEM_SOUNDS), NAMES) {
        Some(path) => load(&path),
        None => {
            log::warn!("no screenshot sound in {SYSTEM_SOUNDS}");
            None
        }
    });
    if let Some(sound) = *sound {
        // SAFETY: `sound` was created by AudioServicesCreateSystemSoundID and is never
        // disposed; playing is asynchronous and thread-safe.
        unsafe { AudioServicesPlaySystemSound(sound) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_macos_has_the_screenshot_sound() {
        let path = find(Path::new(SYSTEM_SOUNDS), NAMES).expect("a screenshot sound");
        assert!(load(&path).is_some());
    }

    #[test]
    fn falls_back_to_older_names_and_then_to_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(find(dir.path(), NAMES), None);
        std::fs::write(dir.path().join("Grab.aif"), b"").unwrap();
        assert_eq!(find(dir.path(), NAMES), Some(dir.path().join("Grab.aif")));
        std::fs::write(dir.path().join("Screen Capture.aif"), b"").unwrap();
        assert_eq!(
            find(dir.path(), NAMES),
            Some(dir.path().join("Screen Capture.aif"))
        );
    }
}
