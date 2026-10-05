//! Global shortcuts: the stored form (`Ctrl+Alt+Shift+Cmd+KeyR`, with W3C key codes),
//! validation and conflicts.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::settings::ShortcutSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ShortcutAction {
    Record,
    CaptureArea,
    CaptureWindow,
}

impl ShortcutAction {
    pub const ALL: [Self; 3] = [Self::Record, Self::CaptureArea, Self::CaptureWindow];

    pub fn label(self) -> &'static str {
        match self {
            Self::Record => "Start or stop recording",
            Self::CaptureArea => "Capture area",
            Self::CaptureWindow => "Capture window",
        }
    }

    pub fn get(self, settings: &ShortcutSettings) -> Option<&str> {
        match self {
            Self::Record => settings.record.as_deref(),
            Self::CaptureArea => settings.capture_area.as_deref(),
            Self::CaptureWindow => settings.capture_window.as_deref(),
        }
    }

    pub fn set(self, settings: &mut ShortcutSettings, shortcut: Option<String>) {
        match self {
            Self::Record => settings.record = shortcut,
            Self::CaptureArea => settings.capture_area = shortcut,
            Self::CaptureWindow => settings.capture_window = shortcut,
        }
    }
}

/// Supported keys: W3C `KeyboardEvent.code` values and how macOS menus show them.
const KEYS: &[(&str, &str)] = &[
    ("KeyA", "A"),
    ("KeyB", "B"),
    ("KeyC", "C"),
    ("KeyD", "D"),
    ("KeyE", "E"),
    ("KeyF", "F"),
    ("KeyG", "G"),
    ("KeyH", "H"),
    ("KeyI", "I"),
    ("KeyJ", "J"),
    ("KeyK", "K"),
    ("KeyL", "L"),
    ("KeyM", "M"),
    ("KeyN", "N"),
    ("KeyO", "O"),
    ("KeyP", "P"),
    ("KeyQ", "Q"),
    ("KeyR", "R"),
    ("KeyS", "S"),
    ("KeyT", "T"),
    ("KeyU", "U"),
    ("KeyV", "V"),
    ("KeyW", "W"),
    ("KeyX", "X"),
    ("KeyY", "Y"),
    ("KeyZ", "Z"),
    ("Digit0", "0"),
    ("Digit1", "1"),
    ("Digit2", "2"),
    ("Digit3", "3"),
    ("Digit4", "4"),
    ("Digit5", "5"),
    ("Digit6", "6"),
    ("Digit7", "7"),
    ("Digit8", "8"),
    ("Digit9", "9"),
    ("F1", "F1"),
    ("F2", "F2"),
    ("F3", "F3"),
    ("F4", "F4"),
    ("F5", "F5"),
    ("F6", "F6"),
    ("F7", "F7"),
    ("F8", "F8"),
    ("F9", "F9"),
    ("F10", "F10"),
    ("F11", "F11"),
    ("F12", "F12"),
    ("F13", "F13"),
    ("F14", "F14"),
    ("F15", "F15"),
    ("F16", "F16"),
    ("F17", "F17"),
    ("F18", "F18"),
    ("F19", "F19"),
    ("F20", "F20"),
    ("Space", "Space"),
    ("Enter", "↩"),
    ("Tab", "⇥"),
    ("Backspace", "⌫"),
    ("Delete", "⌦"),
    ("Home", "↖"),
    ("End", "↘"),
    ("PageUp", "⇞"),
    ("PageDown", "⇟"),
    ("ArrowLeft", "←"),
    ("ArrowRight", "→"),
    ("ArrowUp", "↑"),
    ("ArrowDown", "↓"),
    ("Minus", "-"),
    ("Equal", "="),
    ("BracketLeft", "["),
    ("BracketRight", "]"),
    ("Backslash", "\\"),
    ("Semicolon", ";"),
    ("Quote", "'"),
    ("Comma", ","),
    ("Period", "."),
    ("Slash", "/"),
    ("Backquote", "`"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub cmd: bool,
    /// A W3C key code from the supported list.
    pub key: &'static str,
}

impl Shortcut {
    /// How macOS shows it, such as `⌥⇧⌘R`.
    pub fn symbols(&self) -> String {
        let mut out = String::new();
        for (on, symbol) in [
            (self.ctrl, "⌃"),
            (self.alt, "⌥"),
            (self.shift, "⇧"),
            (self.cmd, "⌘"),
        ] {
            if on {
                out.push_str(symbol);
            }
        }
        out.push_str(key_symbol(self.key));
        out
    }

    fn is_function_key(&self) -> bool {
        self.key.len() > 1 && self.key.starts_with('F') && self.key[1..].parse::<u8>().is_ok()
    }

    const fn new(ctrl: bool, alt: bool, shift: bool, cmd: bool, key: &'static str) -> Self {
        Self {
            ctrl,
            alt,
            shift,
            cmd,
            key,
        }
    }
}

fn key_symbol(key: &str) -> &'static str {
    KEYS.iter()
        .find(|(code, _)| *code == key)
        .map_or("?", |(_, symbol)| symbol)
}

impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
            (self.cmd, "Cmd"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(self.key)
    }
}

impl FromStr for Shortcut {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let key = parts
            .pop()
            .filter(|k| !k.is_empty())
            .ok_or("No key given.")?;
        let key = KEYS
            .iter()
            .find(|(code, _)| *code == key)
            .map(|(code, _)| *code)
            .ok_or_else(|| format!("The key {key} can't be used in a shortcut."))?;
        let mut shortcut = Self::new(false, false, false, false, key);
        for modifier in parts {
            let flag = match modifier.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => &mut shortcut.ctrl,
                "alt" | "option" => &mut shortcut.alt,
                "shift" => &mut shortcut.shift,
                "cmd" | "command" | "super" => &mut shortcut.cmd,
                _ => return Err(format!("Unknown modifier {modifier}.")),
            };
            if *flag {
                return Err(format!("{modifier} appears twice."));
            }
            *flag = true;
        }
        Ok(shortcut)
    }
}

/// Shortcuts macOS or nearly every app already uses.
const RESERVED: &[(Shortcut, &str)] = &[
    (
        Shortcut::new(false, false, true, true, "Digit3"),
        "the macOS screenshot of the screen",
    ),
    (
        Shortcut::new(false, false, true, true, "Digit4"),
        "the macOS screenshot of an area",
    ),
    (
        Shortcut::new(false, false, true, true, "Digit5"),
        "the macOS Screenshot app",
    ),
    (
        Shortcut::new(true, false, true, true, "Digit3"),
        "the macOS screenshot to the clipboard",
    ),
    (
        Shortcut::new(true, false, true, true, "Digit4"),
        "the macOS area screenshot to the clipboard",
    ),
    (
        Shortcut::new(false, false, false, true, "Space"),
        "Spotlight",
    ),
    (
        Shortcut::new(false, true, false, true, "Space"),
        "the Finder search window",
    ),
    (
        Shortcut::new(true, false, false, false, "Space"),
        "switching input sources",
    ),
    (
        Shortcut::new(true, false, false, true, "Space"),
        "Emoji & Symbols",
    ),
    (
        Shortcut::new(true, false, false, true, "KeyQ"),
        "Lock Screen",
    ),
    (
        Shortcut::new(true, false, false, true, "KeyF"),
        "full screen",
    ),
    (
        Shortcut::new(false, true, false, true, "KeyH"),
        "Hide Others",
    ),
    (
        Shortcut::new(false, true, false, true, "KeyD"),
        "showing and hiding the Dock",
    ),
    (Shortcut::new(false, false, true, true, "KeyQ"), "Log Out"),
    (Shortcut::new(false, false, true, true, "KeyZ"), "Redo"),
    (
        Shortcut::new(true, false, false, false, "ArrowUp"),
        "Mission Control",
    ),
    (
        Shortcut::new(true, false, false, false, "ArrowDown"),
        "App Exposé",
    ),
    (
        Shortcut::new(true, false, false, false, "ArrowLeft"),
        "moving between Spaces",
    ),
    (
        Shortcut::new(true, false, false, false, "ArrowRight"),
        "moving between Spaces",
    ),
];

/// Parses `text` for `action` and checks it can work as a global shortcut next to the
/// other actions' shortcuts in `current`.
pub fn check(
    action: ShortcutAction,
    text: &str,
    current: &ShortcutSettings,
) -> Result<Shortcut, String> {
    let shortcut: Shortcut = text.parse()?;
    if !(shortcut.ctrl || shortcut.alt || shortcut.cmd || shortcut.is_function_key()) {
        return Err("Add ⌘, ⌥ or ⌃ so the shortcut doesn't get in the way of typing.".into());
    }
    if shortcut.cmd
        && !(shortcut.ctrl || shortcut.alt || shortcut.shift)
        && !shortcut.is_function_key()
    {
        return Err(format!(
            "{} is an app menu shortcut. Add ⌥, ⌃ or ⇧.",
            shortcut.symbols()
        ));
    }
    if let Some((_, name)) = RESERVED.iter().find(|(reserved, _)| *reserved == shortcut) {
        return Err(format!("{} is already used by {name}.", shortcut.symbols()));
    }
    let taken = ShortcutAction::ALL.into_iter().find(|other| {
        *other != action
            && other
                .get(current)
                .and_then(|s| s.parse::<Shortcut>().ok())
                .is_some_and(|s| s == shortcut)
    });
    if let Some(other) = taken {
        return Err(format!(
            "{} is already used for {}.",
            shortcut.symbols(),
            other.label()
        ));
    }
    Ok(shortcut)
}

/// The shortcuts in `settings` that can be registered, in action order. An action whose
/// shortcut is taken by an earlier one gets an error instead.
pub fn resolve(
    settings: &ShortcutSettings,
) -> Vec<(ShortcutAction, Option<Result<Shortcut, String>>)> {
    let mut earlier = ShortcutSettings {
        record: None,
        capture_area: None,
        capture_window: None,
    };
    ShortcutAction::ALL
        .into_iter()
        .map(|action| {
            let text = action.get(settings).map(str::to_owned);
            let checked = text.as_deref().map(|t| check(action, t, &earlier));
            if let Some(Ok(_)) = &checked {
                action.set(&mut earlier, text);
            }
            (action, checked)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> ShortcutSettings {
        ShortcutSettings::default()
    }

    #[test]
    fn parses_and_prints_the_stored_form() {
        let s: Shortcut = "Alt+Shift+Cmd+KeyR".parse().unwrap();
        assert!(s.alt && s.shift && s.cmd && !s.ctrl);
        assert_eq!(s.key, "KeyR");
        assert_eq!(s.to_string(), "Alt+Shift+Cmd+KeyR");
        assert_eq!(s.symbols(), "⌥⇧⌘R");

        let reordered: Shortcut = "cmd+Option+control+F5".parse().unwrap();
        assert_eq!(reordered.to_string(), "Ctrl+Alt+Cmd+F5");
        assert_eq!(reordered.symbols(), "⌃⌥⌘F5");
        assert_eq!(
            "Ctrl+Shift+ArrowLeft"
                .parse::<Shortcut>()
                .unwrap()
                .symbols(),
            "⌃⇧←"
        );
    }

    #[test]
    fn rejects_malformed_shortcuts() {
        assert!("".parse::<Shortcut>().is_err());
        assert!("Cmd+".parse::<Shortcut>().is_err());
        assert!("Cmd+R".parse::<Shortcut>().is_err(), "codes, not letters");
        assert!("Hyper+KeyR".parse::<Shortcut>().is_err());
        assert!("Cmd+Cmd+KeyR".parse::<Shortcut>().is_err());
        assert!("Cmd+Escape".parse::<Shortcut>().is_err());
    }

    #[test]
    fn every_key_is_a_hotkey_code() {
        for (code, _) in KEYS {
            assert!(
                code.parse::<tauri_plugin_global_shortcut::Code>().is_ok(),
                "{code}"
            );
        }
    }

    #[test]
    fn defaults_are_valid() {
        for (action, checked) in resolve(&defaults()) {
            let shortcut = checked.unwrap().unwrap();
            assert_eq!(Some(shortcut.to_string().as_str()), action.get(&defaults()));
        }
    }

    #[test]
    fn needs_a_modifier_that_typing_does_not_use() {
        let err = check(ShortcutAction::Record, "KeyR", &defaults()).unwrap_err();
        assert!(err.contains("typing"), "{err}");
        assert!(check(ShortcutAction::Record, "Shift+KeyR", &defaults()).is_err());
        assert!(check(ShortcutAction::Record, "F13", &defaults()).is_ok());
        assert!(check(ShortcutAction::Record, "Ctrl+KeyR", &defaults()).is_ok());
    }

    #[test]
    fn command_alone_belongs_to_app_menus() {
        let err = check(ShortcutAction::Record, "Cmd+KeyR", &defaults()).unwrap_err();
        assert!(err.contains("⌘R"), "{err}");
        assert!(check(ShortcutAction::Record, "Cmd+F6", &defaults()).is_ok());
    }

    #[test]
    fn macos_shortcuts_conflict() {
        let err = check(ShortcutAction::CaptureArea, "Shift+Cmd+Digit4", &defaults()).unwrap_err();
        assert_eq!(
            err,
            "⇧⌘4 is already used by the macOS screenshot of an area."
        );
        let err = check(ShortcutAction::Record, "Ctrl+Cmd+Space", &defaults()).unwrap_err();
        assert!(err.contains("Emoji"), "{err}");
    }

    #[test]
    fn two_actions_cannot_share_a_shortcut() {
        let err = check(
            ShortcutAction::CaptureArea,
            "Alt+Shift+Cmd+KeyR",
            &defaults(),
        )
        .unwrap_err();
        assert_eq!(err, "⌥⇧⌘R is already used for Start or stop recording.");
        assert!(
            check(ShortcutAction::Record, "Shift+Alt+Cmd+KeyR", &defaults()).is_ok(),
            "an action may keep its own shortcut"
        );
        let mut settings = defaults();
        settings.record = None;
        assert!(check(ShortcutAction::CaptureArea, "Alt+Shift+Cmd+KeyR", &settings).is_ok());
    }

    #[test]
    fn resolve_reports_bad_and_duplicate_entries() {
        let settings = ShortcutSettings {
            record: Some("Alt+Cmd+KeyK".into()),
            capture_area: Some("Cmd+Alt+KeyK".into()),
            capture_window: Some("nonsense".into()),
        };
        let resolved = resolve(&settings);
        assert!(resolved[0].1.as_ref().unwrap().is_ok());
        assert!(
            resolved[1]
                .1
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap_err()
                .contains("Start or stop recording")
        );
        assert!(resolved[2].1.as_ref().unwrap().is_err());

        let off = ShortcutSettings {
            record: None,
            ..defaults()
        };
        assert!(resolve(&off)[0].1.is_none());
    }
}
