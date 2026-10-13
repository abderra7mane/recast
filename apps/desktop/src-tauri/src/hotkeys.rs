//! Registers the shortcuts from the settings as global hotkeys and runs their actions.

use std::{collections::HashMap, sync::Mutex};

use objc2::{rc::Retained, runtime::AnyObject};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, plugin::TauriPlugin};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut as HotKey, ShortcutState,
};

use crate::{
    settings::SettingsStore,
    shortcuts::{self, Shortcut, ShortcutAction},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutStatus {
    pub action: ShortcutAction,
    /// The stored shortcut, `None` when turned off.
    pub shortcut: Option<String>,
    /// How macOS shows it, such as `⌥⇧⌘R`.
    pub symbols: Option<String>,
    /// Why it doesn't work: a conflict or macOS refusing to register it.
    pub error: Option<String>,
    /// What it takes over from macOS or other apps.
    pub warning: Option<String>,
}

#[derive(Default)]
pub struct Hotkeys {
    state: Mutex<State>,
    /// Held for a whole `apply`, so two never interleave.
    applying: Mutex<()>,
}

#[derive(Default)]
struct State {
    actions: HashMap<u32, ShortcutAction>,
    statuses: Vec<ShortcutStatus>,
    suspended: bool,
}

pub fn hotkey(shortcut: &Shortcut) -> HotKey {
    let mut modifiers = Modifiers::empty();
    for (on, modifier) in [
        (shortcut.ctrl, Modifiers::CONTROL),
        (shortcut.alt, Modifiers::ALT),
        (shortcut.shift, Modifiers::SHIFT),
        (shortcut.cmd, Modifiers::SUPER),
    ] {
        if on {
            modifiers |= modifier;
        }
    }
    let code: Code = shortcut
        .key
        .parse()
        .expect("supported keys are W3C key codes");
    HotKey::new(Some(modifiers), code)
}

pub fn plugin() -> TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app: &AppHandle, shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            let action = app
                .state::<Hotkeys>()
                .state
                .lock()
                .ok()
                .and_then(|s| s.actions.get(&shortcut.id()).copied());
            if let Some(action) = action {
                crate::actions::run(app, action);
            }
        })
        .build()
}

/// Registers the shortcuts in the settings, replacing the current ones.
pub fn apply(app: &AppHandle) -> Vec<ShortcutStatus> {
    let hotkeys = app.state::<Hotkeys>();
    let _applying = hotkeys.applying.lock().unwrap_or_else(|e| e.into_inner());
    let settings = app.state::<SettingsStore>().get().shortcuts;
    // Registering waits for the main thread, where the hotkey handler takes this lock.
    let suspended = {
        let mut state = hotkeys.state.lock().unwrap_or_else(|e| e.into_inner());
        state.actions.clear();
        state.suspended
    };
    let manager = app.global_shortcut();
    if let Err(e) = manager.unregister_all() {
        log::warn!("cannot unregister shortcuts: {e}");
    }
    let system = system_shortcuts();
    let mut actions = HashMap::new();
    let mut statuses = Vec::new();
    for (action, checked) in shortcuts::resolve(&settings) {
        let mut status = ShortcutStatus {
            action,
            shortcut: action.get(&settings).map(str::to_owned),
            symbols: None,
            error: None,
            warning: None,
        };
        match checked {
            None => {}
            Some(Err(e)) => status.error = Some(e),
            Some(Ok(shortcut)) => {
                status.symbols = Some(shortcut.symbols());
                status.warning = shortcuts::warning(&shortcut, &system);
                let key = hotkey(&shortcut);
                if !suspended {
                    match manager.register(key) {
                        Ok(()) => {
                            actions.insert(key.id(), action);
                        }
                        Err(e) => {
                            log::warn!("cannot register {shortcut}: {e}");
                            status.error =
                                Some(format!("macOS didn't accept {}: {e}", shortcut.symbols()));
                        }
                    }
                }
            }
        }
        statuses.push(status);
    }
    let mut state = hotkeys.state.lock().unwrap_or_else(|e| e.into_inner());
    state.actions = actions;
    state.statuses = statuses.clone();
    statuses
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn CopySymbolicHotKeys(hot_keys: *mut *mut NSArray<NSDictionary<NSString, AnyObject>>) -> i32;
}

/// The macOS shortcuts turned on in System Settings, such as the screenshot ones.
fn system_shortcuts() -> Vec<Shortcut> {
    let mut hot_keys = std::ptr::null_mut();
    // SAFETY: `hot_keys` is a valid out pointer; CFArray is toll-free bridged with NSArray.
    let status = unsafe { CopySymbolicHotKeys(&mut hot_keys) };
    // SAFETY: the "Copy" call returns an owned array, or null on failure.
    let Some(hot_keys) = (status == 0)
        .then(|| unsafe { Retained::from_raw(hot_keys) })
        .flatten()
    else {
        log::warn!("cannot read the macOS shortcuts: OSStatus {status}");
        return Vec::new();
    };
    let number = |hot_key: &NSDictionary<NSString, AnyObject>, key: &str| {
        hot_key
            .objectForKey(&NSString::from_str(key))
            .and_then(|value| value.downcast::<NSNumber>().ok())
    };
    hot_keys
        .iter()
        .filter(|hot_key| {
            number(hot_key, "kHISymbolicHotKeyEnabled").is_some_and(|n| n.boolValue())
        })
        .filter_map(|hot_key| {
            Shortcut::from_carbon(
                number(&hot_key, "kHISymbolicHotKeyCode")?.unsignedShortValue(),
                number(&hot_key, "kHISymbolicHotKeyModifiers")?.unsignedIntValue(),
            )
        })
        .collect()
}

pub fn statuses(app: &AppHandle) -> Vec<ShortcutStatus> {
    app.state::<Hotkeys>()
        .state
        .lock()
        .map(|s| s.statuses.clone())
        .unwrap_or_default()
}

/// Turns the hotkeys off while a shortcut is being recorded, so the keys reach the
/// recorder instead of starting an action.
pub fn suspend(app: &AppHandle, suspended: bool) -> Vec<ShortcutStatus> {
    app.state::<Hotkeys>()
        .state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .suspended = suspended;
    apply(app)
}

/// Turns the hotkeys back on if a closed Settings window left them suspended.
pub fn resume(app: &AppHandle) {
    let suspended = app
        .state::<Hotkeys>()
        .state
        .lock()
        .map(|s| s.suspended)
        .unwrap_or(false);
    if suspended {
        // Registering waits for the main thread, so never run it from there.
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || suspend(&app, false));
    }
}

/// The menu accelerator for an action's working shortcut.
pub fn accelerator(app: &AppHandle, action: ShortcutAction) -> Option<String> {
    let settings = app.state::<SettingsStore>().get().shortcuts;
    shortcuts::resolve(&settings)
        .into_iter()
        .find(|(a, _)| *a == action)
        .and_then(|(_, checked)| checked?.ok())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkeys_match_their_shortcuts() {
        let shortcut: Shortcut = "Alt+Shift+Cmd+KeyR".parse().unwrap();
        let key = hotkey(&shortcut);
        assert_eq!(key.key, Code::KeyR);
        assert_eq!(
            key.mods,
            Modifiers::ALT | Modifiers::SHIFT | Modifiers::SUPER
        );
        let parsed: HotKey = "alt+shift+super+KeyR".parse().unwrap();
        assert_eq!(key.id(), parsed.id());

        let ctrl: Shortcut = "Ctrl+F5".parse().unwrap();
        assert_eq!(hotkey(&ctrl).mods, Modifiers::CONTROL);
    }

    #[test]
    fn reads_the_macos_shortcuts() {
        let system = system_shortcuts();
        assert!(!system.is_empty());
        let globe_only = |s: &Shortcut| s.key.starts_with("Key") && !(s.ctrl || s.alt || s.cmd);
        assert!(!system.iter().any(globe_only), "{system:?}");
    }
}
