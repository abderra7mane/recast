//! The UI theme. The webview turns themes into CSS variables; Rust keeps the choice,
//! hands the variables to every window before its first paint, and gives the native
//! window the theme's light or dark appearance and background.

use std::{
    collections::BTreeMap,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State, WebviewWindow, Window};
use tauri_specta::Event;

use crate::settings::{AppSettings, SettingsStore};

pub const DEFAULT_THEME: &str = "recast-dark";
pub const DEFAULT_LIGHT_THEME: &str = "recast-light";
/// Theme files are a few kilobytes; anything much bigger is not one.
const MAX_THEME_FILE: u64 = 256 * 1024;
const THEME_FORMAT: &str = "recast-theme";
const MAX_NAME_CHARS: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ThemeBase {
    Light,
    Dark,
}

/// A theme the user made or imported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomTheme {
    pub id: String,
    pub name: String,
    pub base: ThemeBase,
    /// The built-in theme it was copied from; resetting a color goes back to its value.
    #[serde(default)]
    pub based_on: Option<String>,
    /// Token name to `#rrggbb` or `#rrggbbaa`.
    pub colors: BTreeMap<String, String>,
}

/// A theme as CSS custom properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ThemeStyle {
    pub base: ThemeBase,
    /// `--t-*` property to `#rrggbb` or `#rrggbbaa`.
    pub vars: BTreeMap<String, String>,
}

/// The styles windows apply: `dark` alone, or `light` and `dark` following macOS.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppliedTheme {
    pub follow_system: bool,
    pub light: ThemeStyle,
    pub dark: ThemeStyle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct AppearanceSettings {
    /// `system`, or the id of a built-in or custom theme.
    pub theme: String,
    /// The themes `system` uses in light and dark mode.
    pub system_light: String,
    pub system_dark: String,
    pub custom_themes: Vec<CustomTheme>,
    /// The chosen theme as resolved by the webview. `None` leaves the stylesheet's
    /// defaults, which are Recast Dark.
    pub applied: Option<AppliedTheme>,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: DEFAULT_THEME.into(),
            system_light: DEFAULT_LIGHT_THEME.into(),
            system_dark: DEFAULT_THEME.into(),
            custom_themes: Vec::new(),
            applied: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceChanged {
    pub applied: Option<AppliedTheme>,
    /// The window that made the change.
    pub source: String,
}

/// `#rrggbb` or `#rrggbbaa`, the form the webview stores colors in.
fn is_color(value: &str) -> bool {
    value
        .strip_prefix('#')
        .is_some_and(|hex| matches!(hex.len(), 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit()))
}

fn is_var(name: &str) -> bool {
    name.strip_prefix("--t-").is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    })
}

fn rgb(value: &str) -> Option<(u8, u8, u8)> {
    if !is_color(value) {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&value[1 + 2 * i..3 + 2 * i], 16).ok();
    Some((channel(0)?, channel(1)?, channel(2)?))
}

impl ThemeStyle {
    fn validate(&self) -> Result<(), String> {
        for (name, value) in &self.vars {
            if !is_var(name) {
                return Err(format!("{name} is not a theme variable"));
            }
            if !is_color(value) {
                return Err(format!("{name} has an invalid color {value:?}"));
            }
        }
        Ok(())
    }

    fn background(&self) -> Option<(u8, u8, u8)> {
        self.vars.get("--t-background").and_then(|v| rgb(v))
    }
}

impl AppearanceSettings {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(applied) = &self.applied {
            applied.light.validate()?;
            applied.dark.validate()?;
        }
        let mut ids = std::collections::HashSet::new();
        for theme in &self.custom_themes {
            if theme.id.trim().is_empty() || !ids.insert(theme.id.as_str()) {
                return Err(format!("duplicate or empty theme id {:?}", theme.id));
            }
            let name = theme.name.trim();
            if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
                return Err(format!(
                    "theme names need 1 to {MAX_NAME_CHARS} characters: {:?}",
                    theme.name
                ));
            }
            if let Some((token, value)) = theme.colors.iter().find(|(_, v)| !is_color(v)) {
                return Err(format!(
                    "{} has an invalid {token} color {value:?}",
                    theme.name
                ));
            }
        }
        Ok(())
    }
}

/// The base a window takes regardless of macOS, or `None` to follow it.
fn fixed_base(applied: Option<&AppliedTheme>) -> Option<ThemeBase> {
    match applied {
        None => Some(ThemeBase::Dark),
        Some(applied) if applied.follow_system => None,
        Some(applied) => Some(applied.dark.base),
    }
}

/// A script that applies `applied` to the page before anything renders. It runs when
/// the document element exists and before the page's own scripts.
pub fn boot_script(applied: &AppliedTheme) -> String {
    let theme = serde_json::to_string(applied).unwrap_or_else(|_| "null".into());
    format!(
        r#"(() => {{
  const theme = {theme};
  if (!theme) return;
  window.__RECAST_THEME__ = theme;
  const dark =
    !theme.followSystem || window.matchMedia("(prefers-color-scheme: dark)").matches;
  const style = dark ? theme.dark : theme.light;
  const root = document.documentElement;
  if (!root) return;
  for (const [name, value] of Object.entries(style.vars)) root.style.setProperty(name, value);
  root.classList.toggle("dark", style.base === "dark");
  root.style.colorScheme = style.base;
}})();"#
    )
}

/// Matches the native window to the theme: its appearance, which colors the title bar,
/// scrollbars and system controls, and its background, seen before the page paints.
pub fn apply_to_window(window: &Window, applied: Option<&AppliedTheme>) {
    let base = fixed_base(applied);
    let backgrounds = applied.map(|a| (a.light.background(), a.dark.background()));
    let target = window.clone();
    let result = window.run_on_main_thread(move || {
        let Ok(pointer) = target.ns_window() else {
            return;
        };
        // SAFETY: Tauri's NSWindow pointer is valid on the main thread while the window
        // exists, which `target` guarantees.
        let ns_window = unsafe { &*pointer.cast::<objc2_app_kit::NSWindow>() };
        crate::appkit::set_dark_appearance(ns_window, base.map(|b| b == ThemeBase::Dark));
        if let Some((light, dark)) = backgrounds {
            let shown = if crate::appkit::is_dark(ns_window) {
                dark
            } else {
                light
            };
            if let Some((r, g, b)) = shown {
                let channel = |c: u8| f64::from(c) / 255.0;
                ns_window.setBackgroundColor(Some(&crate::appkit::color(
                    channel(r),
                    channel(g),
                    channel(b),
                    1.0,
                )));
            }
        }
    });
    if let Err(e) = result {
        log::warn!("cannot theme the {} window: {e}", window.label());
    }
}

pub fn apply_to_all(app: &AppHandle, applied: Option<&AppliedTheme>) {
    for window in app.webview_windows().values() {
        apply_to_window(&window.as_ref().window(), applied);
    }
}

/// Settings are written at most this often while edits come many times a second.
const SAVE_INTERVAL: Duration = Duration::from_millis(300);
static SAVE_QUEUED: AtomicBool = AtomicBool::new(false);

/// Writes the settings `SAVE_INTERVAL` after the first unsaved change, so a long drag
/// writes a few times a second; quitting writes them too.
fn save_soon(app: &AppHandle) {
    if SAVE_QUEUED.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(SAVE_INTERVAL);
        SAVE_QUEUED.store(false, Ordering::SeqCst);
        if let Err(e) = app.state::<SettingsStore>().save() {
            log::warn!("{e}");
        }
    });
}

/// Whether the native window looks the same for both.
fn same_native(a: Option<&AppliedTheme>, b: Option<&AppliedTheme>) -> bool {
    let backgrounds =
        |t: Option<&AppliedTheme>| t.map(|t| (t.light.background(), t.dark.background()));
    fixed_base(a) == fixed_base(b) && backgrounds(a) == backgrounds(b)
}

/// Shows `applied` everywhere: native windows, and every webview but `source`, which
/// already shows it.
fn restyle(
    app: &AppHandle,
    source: &str,
    previous: Option<&AppliedTheme>,
    applied: Option<AppliedTheme>,
) {
    if previous == applied.as_ref() {
        return;
    }
    if !same_native(previous, applied.as_ref()) {
        apply_to_all(app, applied.as_ref());
    }
    let _ = AppearanceChanged {
        applied,
        source: source.to_owned(),
    }
    .emit(app);
}

/// Changes the appearance and restyles every open window. A sync command, so changes
/// apply in the order they were sent.
#[tauri::command]
#[specta::specta]
pub fn set_appearance(
    app: AppHandle,
    window: WebviewWindow,
    settings: State<'_, SettingsStore>,
    appearance: AppearanceSettings,
) -> Result<AppSettings, String> {
    appearance.validate()?;
    let previous = settings.get().appearance.applied;
    let saved = settings.update_unsaved(|s| s.appearance = appearance)?;
    save_soon(&app);
    restyle(
        &app,
        window.label(),
        previous.as_ref(),
        saved.appearance.applied.clone(),
    );
    Ok(saved)
}

/// Replaces the resolved theme, if the appearance is still `seen`; a window resolving
/// the saved choice again never overwrites a newer choice.
#[tauri::command]
#[specta::specta]
pub fn refresh_applied_theme(
    app: AppHandle,
    window: WebviewWindow,
    settings: State<'_, SettingsStore>,
    seen: AppearanceSettings,
    applied: AppliedTheme,
) -> Result<bool, String> {
    let next = AppearanceSettings {
        applied: Some(applied),
        ..seen.clone()
    };
    next.validate()?;
    let mut previous = None;
    settings.update_unsaved(|s| {
        if s.appearance == seen {
            previous = Some(std::mem::replace(&mut s.appearance, next));
        }
    })?;
    let Some(previous) = previous else {
        return Ok(false);
    };
    save_soon(&app);
    let applied = settings.get().appearance.applied;
    restyle(&app, window.label(), previous.applied.as_ref(), applied);
    Ok(true)
}

fn require_json(path: &Path) -> Result<(), String> {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
    {
        Ok(())
    } else {
        Err("Theme files end in .json.".into())
    }
}

pub fn write_theme_file(path: &Path, contents: &str) -> Result<(), String> {
    require_json(path)?;
    let json: serde_json::Value =
        serde_json::from_str(contents).map_err(|e| format!("not a theme: {e}"))?;
    if json.get("format").and_then(|f| f.as_str()) != Some(THEME_FORMAT) {
        return Err("not a theme".into());
    }
    std::fs::write(path, contents).map_err(|e| format!("Cannot save {}: {e}", path.display()))
}

pub fn read_theme_file(path: &Path) -> Result<String, String> {
    require_json(path)?;
    let size = std::fs::metadata(path)
        .map_err(|e| format!("Cannot open {}: {e}", path.display()))?
        .len();
    if size > MAX_THEME_FILE {
        return Err(format!(
            "{} is too big for a theme ({} KB).",
            path.display(),
            size / 1024
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
    String::from_utf8(bytes).map_err(|_| format!("{} is not a text file.", path.display()))
}

/// Writes a theme file the webview made.
#[tauri::command]
#[specta::specta]
pub async fn export_theme(path: String, contents: String) -> Result<(), String> {
    write_theme_file(Path::new(&path), &contents)
}

/// Reads a theme file for the webview to check and import.
#[tauri::command]
#[specta::specta]
pub async fn import_theme(path: String) -> Result<String, String> {
    read_theme_file(Path::new(&path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(base: ThemeBase, background: &str) -> ThemeStyle {
        ThemeStyle {
            base,
            vars: BTreeMap::from([
                ("--t-background".into(), background.into()),
                ("--t-foreground".into(), "#fafafa".into()),
            ]),
        }
    }

    fn applied(follow_system: bool) -> AppliedTheme {
        AppliedTheme {
            follow_system,
            light: style(ThemeBase::Light, "#ffffff"),
            dark: style(ThemeBase::Dark, "#282a36"),
        }
    }

    #[test]
    fn defaults_keep_recast_dark() {
        let appearance = AppearanceSettings::default();
        assert_eq!(appearance.theme, "recast-dark");
        assert_eq!(appearance.system_light, "recast-light");
        assert_eq!(appearance.system_dark, "recast-dark");
        assert!(appearance.applied.is_none());
        assert_eq!(fixed_base(None), Some(ThemeBase::Dark));
        assert!(appearance.validate().is_ok());
    }

    #[test]
    fn the_native_appearance_follows_the_theme_base() {
        assert_eq!(fixed_base(Some(&applied(true))), None);
        let mut light = applied(false);
        light.dark = style(ThemeBase::Light, "#fdf6e3");
        assert_eq!(fixed_base(Some(&light)), Some(ThemeBase::Light));
        assert_eq!(fixed_base(Some(&applied(false))), Some(ThemeBase::Dark));
        assert_eq!(applied(true).dark.background(), Some((0x28, 0x2a, 0x36)));
    }

    #[test]
    fn only_base_or_background_changes_restyle_native_windows() {
        let theme = applied(false);
        let mut accent = theme.clone();
        accent
            .dark
            .vars
            .insert("--t-accent".into(), "#ff0000".into());
        assert!(same_native(Some(&theme), Some(&accent)));
        let mut background = theme.clone();
        background
            .dark
            .vars
            .insert("--t-background".into(), "#000000".into());
        assert!(!same_native(Some(&theme), Some(&background)));
        assert!(!same_native(Some(&theme), Some(&applied(true))));
        assert!(!same_native(None, Some(&theme)));
    }

    #[test]
    fn styles_with_foreign_variables_or_colors_are_refused() {
        let with = |name: &str, value: &str| {
            let mut theme = applied(false);
            theme.dark.vars.insert(name.into(), value.into());
            AppearanceSettings {
                applied: Some(theme),
                ..Default::default()
            }
            .validate()
        };
        assert!(with("--t-accent", "#0a84ff").is_ok());
        assert!(with("--t-accent", "#0a84ff80").is_ok());
        assert!(with("color", "#000000").is_err());
        assert!(with("--t-Accent", "#000000").is_err());
        assert!(with("--t-border", "red; background: url(x)").is_err());
        assert!(with("--t-border", "#12345").is_err());
    }

    #[test]
    fn custom_themes_need_unique_ids_names_and_colors() {
        let theme = CustomTheme {
            id: "custom-1".into(),
            name: "Mine".into(),
            base: ThemeBase::Light,
            based_on: Some("nord".into()),
            colors: BTreeMap::from([("background".into(), "#eceff4".into())]),
        };
        let with = |themes: Vec<CustomTheme>| AppearanceSettings {
            custom_themes: themes,
            ..Default::default()
        };
        assert!(with(vec![theme.clone()]).validate().is_ok());
        assert!(with(vec![theme.clone(), theme.clone()]).validate().is_err());
        let unnamed = CustomTheme {
            name: "  ".into(),
            ..theme.clone()
        };
        assert!(with(vec![unnamed]).validate().is_err());
        let mut bad = theme.clone();
        bad.colors.insert("accent".into(), "blue".into());
        assert!(with(vec![bad]).validate().is_err());
    }

    #[test]
    fn the_boot_script_carries_the_styles() {
        let script = boot_script(&applied(true));
        assert!(script.contains(r##""--t-background":"#282a36""##));
        assert!(script.contains(r#""followSystem":true"#));
        assert!(script.contains("prefers-color-scheme: dark"));
        assert!(script.contains("classList.toggle(\"dark\""));
    }

    #[test]
    fn appearance_persists_with_the_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(crate::settings::FILE_NAME);
        let store = SettingsStore::load(&path);
        let appearance = AppearanceSettings {
            theme: "custom-1".into(),
            system_light: "github-light".into(),
            system_dark: "dracula".into(),
            custom_themes: vec![CustomTheme {
                id: "custom-1".into(),
                name: "Mine".into(),
                base: ThemeBase::Dark,
                based_on: None,
                colors: BTreeMap::from([("background".into(), "#101010".into())]),
            }],
            applied: Some(applied(false)),
        };
        store.update(|s| s.appearance = appearance.clone()).unwrap();
        assert_eq!(SettingsStore::load(&path).get().appearance, appearance);
    }

    #[test]
    fn older_settings_get_the_default_appearance() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(crate::settings::FILE_NAME);
        std::fs::write(&path, r#"{"version":2,"onboardingCompleted":true}"#).unwrap();
        let settings = SettingsStore::load(&path).get();
        assert_eq!(settings.appearance, AppearanceSettings::default());
        assert!(settings.onboarding_completed);
    }

    #[test]
    fn theme_files_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Mine.json");
        let contents = r##"{"format":"recast-theme","version":1,"name":"Mine","base":"dark","colors":{"background":"#101010"}}"##;
        write_theme_file(&path, contents).unwrap();
        assert_eq!(read_theme_file(&path).unwrap(), contents);
    }

    #[test]
    fn only_json_theme_files_are_read_and_written() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            write_theme_file(&dir.path().join("x.txt"), r#"{"format":"recast-theme"}"#).is_err()
        );
        assert!(write_theme_file(&dir.path().join("x.json"), r#"{"format":"other"}"#).is_err());
        assert!(write_theme_file(&dir.path().join("x.json"), "not json").is_err());
        assert!(!dir.path().join("x.json").exists());

        let text = dir.path().join("notes.txt");
        std::fs::write(&text, "{}").unwrap();
        assert!(read_theme_file(&text).is_err());

        let big = dir.path().join("big.json");
        std::fs::write(&big, vec![b' '; MAX_THEME_FILE as usize + 1]).unwrap();
        assert!(read_theme_file(&big).unwrap_err().contains("too big"));

        let binary = dir.path().join("binary.json");
        std::fs::write(&binary, [0xff, 0xfe, 0x00]).unwrap();
        assert!(read_theme_file(&binary).is_err());
        assert!(read_theme_file(&dir.path().join("missing.json")).is_err());
    }
}
