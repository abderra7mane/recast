//! Global shortcuts: the stored form (`Ctrl+Alt+Shift+Cmd+KeyR`, with W3C key codes),
//! validation and conflicts.

use std::{fmt, str::FromStr};

use recast_capture::picker::PickMode;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::settings::ShortcutSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ShortcutAction {
    RecordArea,
    RecordWindow,
    RecordDisplay,
    CaptureArea,
    CaptureWindow,
    CaptureDisplay,
}

impl ShortcutAction {
    pub const ALL: [Self; 6] = [
        Self::RecordArea,
        Self::RecordWindow,
        Self::RecordDisplay,
        Self::CaptureArea,
        Self::CaptureWindow,
        Self::CaptureDisplay,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::RecordArea => "Record area",
            Self::RecordWindow => "Record window",
            Self::RecordDisplay => "Record display",
            Self::CaptureArea => "Capture area",
            Self::CaptureWindow => "Capture window",
            Self::CaptureDisplay => "Capture display",
        }
    }

    /// Whether the action records; its shortcut also stops a running recording.
    pub fn records(self) -> bool {
        matches!(
            self,
            Self::RecordArea | Self::RecordWindow | Self::RecordDisplay
        )
    }

    pub fn mode(self) -> PickMode {
        match self {
            Self::RecordArea | Self::CaptureArea => PickMode::Area,
            Self::RecordWindow | Self::CaptureWindow => PickMode::Window,
            Self::RecordDisplay | Self::CaptureDisplay => PickMode::Display,
        }
    }

    fn slot(self, settings: &ShortcutSettings) -> &Option<String> {
        match self {
            Self::RecordArea => &settings.record_area,
            Self::RecordWindow => &settings.record_window,
            Self::RecordDisplay => &settings.record_display,
            Self::CaptureArea => &settings.capture_area,
            Self::CaptureWindow => &settings.capture_window,
            Self::CaptureDisplay => &settings.capture_display,
        }
    }

    pub fn get(self, settings: &ShortcutSettings) -> Option<&str> {
        self.slot(settings).as_deref()
    }

    pub fn set(self, settings: &mut ShortcutSettings, shortcut: Option<String>) {
        let slot = match self {
            Self::RecordArea => &mut settings.record_area,
            Self::RecordWindow => &mut settings.record_window,
            Self::RecordDisplay => &mut settings.record_display,
            Self::CaptureArea => &mut settings.capture_area,
            Self::CaptureWindow => &mut settings.capture_window,
            Self::CaptureDisplay => &mut settings.capture_display,
        };
        *slot = shortcut;
    }
}

/// Supported keys: W3C `KeyboardEvent.code` values, how macOS menus show them and their
/// macOS virtual key codes.
const KEYS: &[(&str, &str, u16)] = &[
    ("KeyA", "A", 0x00),
    ("KeyB", "B", 0x0b),
    ("KeyC", "C", 0x08),
    ("KeyD", "D", 0x02),
    ("KeyE", "E", 0x0e),
    ("KeyF", "F", 0x03),
    ("KeyG", "G", 0x05),
    ("KeyH", "H", 0x04),
    ("KeyI", "I", 0x22),
    ("KeyJ", "J", 0x26),
    ("KeyK", "K", 0x28),
    ("KeyL", "L", 0x25),
    ("KeyM", "M", 0x2e),
    ("KeyN", "N", 0x2d),
    ("KeyO", "O", 0x1f),
    ("KeyP", "P", 0x23),
    ("KeyQ", "Q", 0x0c),
    ("KeyR", "R", 0x0f),
    ("KeyS", "S", 0x01),
    ("KeyT", "T", 0x11),
    ("KeyU", "U", 0x20),
    ("KeyV", "V", 0x09),
    ("KeyW", "W", 0x0d),
    ("KeyX", "X", 0x07),
    ("KeyY", "Y", 0x10),
    ("KeyZ", "Z", 0x06),
    ("Digit0", "0", 0x1d),
    ("Digit1", "1", 0x12),
    ("Digit2", "2", 0x13),
    ("Digit3", "3", 0x14),
    ("Digit4", "4", 0x15),
    ("Digit5", "5", 0x17),
    ("Digit6", "6", 0x16),
    ("Digit7", "7", 0x1a),
    ("Digit8", "8", 0x1c),
    ("Digit9", "9", 0x19),
    ("F1", "F1", 0x7a),
    ("F2", "F2", 0x78),
    ("F3", "F3", 0x63),
    ("F4", "F4", 0x76),
    ("F5", "F5", 0x60),
    ("F6", "F6", 0x61),
    ("F7", "F7", 0x62),
    ("F8", "F8", 0x64),
    ("F9", "F9", 0x65),
    ("F10", "F10", 0x6d),
    ("F11", "F11", 0x67),
    ("F12", "F12", 0x6f),
    ("F13", "F13", 0x69),
    ("F14", "F14", 0x6b),
    ("F15", "F15", 0x71),
    ("F16", "F16", 0x6a),
    ("F17", "F17", 0x40),
    ("F18", "F18", 0x4f),
    ("F19", "F19", 0x50),
    ("F20", "F20", 0x5a),
    ("Space", "Space", 0x31),
    ("Enter", "↩", 0x24),
    ("Tab", "⇥", 0x30),
    ("Backspace", "⌫", 0x33),
    ("Delete", "⌦", 0x75),
    ("Home", "↖", 0x73),
    ("End", "↘", 0x77),
    ("PageUp", "⇞", 0x74),
    ("PageDown", "⇟", 0x79),
    ("ArrowLeft", "←", 0x7b),
    ("ArrowRight", "→", 0x7c),
    ("ArrowUp", "↑", 0x7e),
    ("ArrowDown", "↓", 0x7d),
    ("Minus", "-", 0x1b),
    ("Equal", "=", 0x18),
    ("BracketLeft", "[", 0x21),
    ("BracketRight", "]", 0x1e),
    ("Backslash", "\\", 0x2a),
    ("Semicolon", ";", 0x29),
    ("Quote", "'", 0x27),
    ("Comma", ",", 0x2b),
    ("Period", ".", 0x2f),
    ("Slash", "/", 0x2c),
    ("Backquote", "`", 0x32),
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

    /// Whether the key reports the fn flag by itself, as function and navigation keys do.
    fn sets_fn_flag(&self) -> bool {
        self.is_function_key()
            || matches!(
                self.key,
                "ArrowLeft"
                    | "ArrowRight"
                    | "ArrowUp"
                    | "ArrowDown"
                    | "Home"
                    | "End"
                    | "PageUp"
                    | "PageDown"
                    | "Delete"
            )
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
        .find(|(code, ..)| *code == key)
        .map_or("?", |(_, symbol, _)| symbol)
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
            .find(|(code, ..)| *code == key)
            .map(|(code, ..)| *code)
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

impl Shortcut {
    /// A shortcut from a Carbon virtual key code and modifier flags, if Recast can
    /// register it: its key is supported and it doesn't need the Globe (fn) key.
    pub fn from_carbon(key_code: u16, modifiers: u32) -> Option<Self> {
        const CMD: u32 = 0x100;
        const SHIFT: u32 = 0x200;
        const OPTION: u32 = 0x800;
        const CONTROL: u32 = 0x1000;
        const FN: u32 = 0x20000;
        let key = KEYS.iter().find(|(.., code)| *code == key_code)?.0;
        let shortcut = Self::new(
            modifiers & CONTROL != 0,
            modifiers & OPTION != 0,
            modifiers & SHIFT != 0,
            modifiers & CMD != 0,
            key,
        );
        (modifiers & FN == 0 || shortcut.sets_fn_flag()).then_some(shortcut)
    }
}

/// Shortcuts that apps or the Apple menu use, beyond those macOS reports itself.
const APP_SHORTCUTS: &[(Shortcut, &str)] = &[
    (Shortcut::new(false, false, true, true, "KeyZ"), "Redo"),
    (Shortcut::new(false, false, true, true, "KeyQ"), "Log Out"),
    (
        Shortcut::new(true, false, false, true, "KeyF"),
        "full screen",
    ),
    (
        Shortcut::new(false, true, false, true, "KeyH"),
        "Hide Others",
    ),
    (
        Shortcut::new(true, false, false, true, "Space"),
        "Emoji & Symbols",
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

/// What `shortcut` takes over from macOS or other apps, given the macOS shortcuts that
/// are turned on.
pub fn warning(shortcut: &Shortcut, system: &[Shortcut]) -> Option<String> {
    let symbols = shortcut.symbols();
    if system.contains(shortcut) {
        return Some(format!(
            "macOS uses {symbols} too. Turn it off in System Settings › Keyboard › Keyboard Shortcuts so Recast gets it."
        ));
    }
    if let Some((_, name)) = APP_SHORTCUTS.iter().find(|(s, _)| s == shortcut) {
        return Some(format!(
            "{symbols} is {name} in most apps. Recast takes it over in every app."
        ));
    }
    let modifiers = [shortcut.ctrl, shortcut.alt, shortcut.shift, shortcut.cmd];
    if modifiers.into_iter().filter(|on| *on).count() == 1 && !shortcut.is_function_key() {
        return Some(format!(
            "Apps often use {symbols} themselves. Recast takes it over in every app."
        ));
    }
    None
}

/// The shortcuts in `settings` that can be registered, in action order. An action whose
/// shortcut is taken by an earlier one gets an error instead.
pub fn resolve(
    settings: &ShortcutSettings,
) -> Vec<(ShortcutAction, Option<Result<Shortcut, String>>)> {
    let mut earlier = ShortcutSettings::none();
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
        for (code, ..) in KEYS {
            assert!(
                code.parse::<tauri_plugin_global_shortcut::Code>().is_ok(),
                "{code}"
            );
        }
    }

    #[test]
    fn defaults_are_valid() {
        let mut set = Vec::new();
        for (action, checked) in resolve(&defaults()) {
            let Some(checked) = checked else {
                continue;
            };
            let shortcut = checked.unwrap();
            assert_eq!(Some(shortcut.to_string().as_str()), action.get(&defaults()));
            set.push((action, shortcut.symbols()));
        }
        assert_eq!(
            set,
            [
                (ShortcutAction::RecordArea, "⌥⇧⌘R".to_string()),
                (ShortcutAction::CaptureArea, "⌥⇧⌘S".to_string())
            ]
        );
    }

    #[test]
    fn every_action_has_its_own_slot_and_mode() {
        let mut settings = ShortcutSettings::none();
        for (i, action) in ShortcutAction::ALL.into_iter().enumerate() {
            action.set(&mut settings, Some(format!("Alt+Cmd+Digit{i}")));
        }
        for (i, action) in ShortcutAction::ALL.into_iter().enumerate() {
            assert_eq!(
                action.get(&settings),
                Some(format!("Alt+Cmd+Digit{i}").as_str())
            );
        }
        assert!(
            resolve(&settings)
                .iter()
                .all(|(_, c)| matches!(c, Some(Ok(_))))
        );
        let modes: Vec<_> = ShortcutAction::ALL.map(|a| (a.records(), a.mode())).into();
        assert_eq!(
            modes,
            [
                (true, PickMode::Area),
                (true, PickMode::Window),
                (true, PickMode::Display),
                (false, PickMode::Area),
                (false, PickMode::Window),
                (false, PickMode::Display),
            ]
        );
    }

    #[test]
    fn needs_a_modifier_that_typing_does_not_use() {
        let err = check(ShortcutAction::RecordArea, "KeyR", &defaults()).unwrap_err();
        assert!(err.contains("typing"), "{err}");
        assert!(check(ShortcutAction::RecordArea, "Shift+KeyR", &defaults()).is_err());
        assert!(check(ShortcutAction::RecordArea, "F13", &defaults()).is_ok());
        assert!(check(ShortcutAction::RecordArea, "Ctrl+KeyR", &defaults()).is_ok());
    }

    #[test]
    fn allows_any_shortcut_with_a_modifier_typing_does_not_use() {
        for allowed in [
            "Cmd+KeyR",
            "Alt+Space",
            "Ctrl+Space",
            "Shift+Cmd+Digit4",
            "Cmd+F6",
        ] {
            assert!(
                check(ShortcutAction::RecordWindow, allowed, &defaults()).is_ok(),
                "{allowed}"
            );
        }
    }

    #[test]
    fn carbon_shortcuts_map_to_key_codes() {
        let screenshot = Shortcut::from_carbon(0x15, 0x100 | 0x200).unwrap();
        assert_eq!(screenshot.to_string(), "Shift+Cmd+Digit4");
        let mission_control = Shortcut::from_carbon(0x7e, 0x1000 | 0x20000).unwrap();
        assert_eq!(mission_control.to_string(), "Ctrl+ArrowUp");
        assert_eq!(Shortcut::from_carbon(0x35, 0x100), None, "Escape");
        assert_eq!(Shortcut::from_carbon(0x0c, 0x20000), None, "Globe+Q");
        let show_desktop = Shortcut::from_carbon(0x67, 0x20000).unwrap();
        assert_eq!(show_desktop.to_string(), "F11");
    }

    #[test]
    fn every_key_has_its_own_key_code() {
        let mut codes: Vec<_> = KEYS.iter().map(|(.., code)| *code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), KEYS.len());
    }

    #[test]
    fn warns_about_shortcuts_taken_from_macos_and_apps() {
        let parse = |text: &str| text.parse::<Shortcut>().unwrap();
        let system = [parse("Shift+Cmd+Digit4")];
        assert_eq!(
            warning(&parse("Shift+Cmd+Digit4"), &system).unwrap(),
            "macOS uses ⇧⌘4 too. Turn it off in System Settings › Keyboard › Keyboard Shortcuts so Recast gets it."
        );
        assert_eq!(warning(&parse("Shift+Cmd+Digit4"), &[]), None);
        assert_eq!(
            warning(&parse("Shift+Cmd+KeyZ"), &[]).unwrap(),
            "⇧⌘Z is Redo in most apps. Recast takes it over in every app."
        );
        for single in ["Cmd+KeyR", "Alt+Space", "Ctrl+KeyA"] {
            assert!(
                warning(&parse(single), &[])
                    .unwrap()
                    .starts_with("Apps often use"),
                "{single}"
            );
        }
        for quiet in ["Alt+Shift+Cmd+KeyR", "Ctrl+Alt+KeyW", "Cmd+F6", "F13"] {
            assert_eq!(warning(&parse(quiet), &[]), None, "{quiet}");
        }
    }

    #[test]
    fn two_actions_cannot_share_a_shortcut() {
        let err = check(
            ShortcutAction::CaptureArea,
            "Alt+Shift+Cmd+KeyR",
            &defaults(),
        )
        .unwrap_err();
        assert_eq!(err, "⌥⇧⌘R is already used for Record area.");
        assert!(
            check(
                ShortcutAction::RecordArea,
                "Shift+Alt+Cmd+KeyR",
                &defaults()
            )
            .is_ok(),
            "an action may keep its own shortcut"
        );
        let err = check(
            ShortcutAction::RecordDisplay,
            "Alt+Shift+Cmd+KeyS",
            &defaults(),
        )
        .unwrap_err();
        assert_eq!(err, "⌥⇧⌘S is already used for Capture area.");
        let mut settings = defaults();
        settings.record_area = None;
        assert!(check(ShortcutAction::CaptureArea, "Alt+Shift+Cmd+KeyR", &settings).is_ok());
    }

    #[test]
    fn resolve_reports_bad_and_duplicate_entries() {
        let settings = ShortcutSettings {
            record_area: Some("Alt+Cmd+KeyK".into()),
            capture_area: Some("Cmd+Alt+KeyK".into()),
            capture_window: Some("nonsense".into()),
            ..ShortcutSettings::none()
        };
        let resolved = resolve(&settings);
        assert!(resolved[0].1.as_ref().unwrap().is_ok());
        assert!(resolved[1].1.is_none());
        assert!(
            resolved[3]
                .1
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap_err()
                .contains("Record area")
        );
        assert!(resolved[4].1.as_ref().unwrap().is_err());

        let off = ShortcutSettings {
            record_area: None,
            ..defaults()
        };
        assert!(resolve(&off)[0].1.is_none());
    }
}
