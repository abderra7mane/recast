//! The menu bar icon and its menu. While recording, clicking the icon stops the
//! recording; the menu opens with a right click and starts with Stop.

use std::{path::Path, sync::Mutex};

use tauri::{
    AppHandle, Manager,
    image::Image,
    menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
};

use crate::{
    editor::{self, ProjectSummary},
    flow::{Flow, Phase},
    hotkeys,
    settings::SettingsStore,
    shortcuts::ShortcutAction,
    windows,
};

const TRAY_ID: &str = "recast";
const RECENT_PREFIX: &str = "recent:";
pub const RECENT_COUNT: usize = 5;
/// Menu bar icons are 18 points tall; drawn at 2× for Retina displays.
const ICON_PX: usize = 36;

#[derive(Debug, Clone, PartialEq)]
pub struct Recent {
    pub path: String,
    pub label: String,
}

/// The newest `count` finished recordings, for the Recent Recordings menu.
pub fn recent(projects: Vec<ProjectSummary>, count: usize) -> Vec<Recent> {
    let mut projects = projects;
    projects.sort_by(|a, b| b.created_at_unix_ms.total_cmp(&a.created_at_unix_ms));
    projects
        .into_iter()
        .take(count)
        .map(|p| {
            let seconds = (p.duration_ms / 1000.0).round().max(0.0) as u64;
            Recent {
                label: format!("{}  ({}:{:02})", p.name, seconds / 60, seconds % 60),
                path: p.path,
            }
        })
        .collect()
}

/// Paths behind the Recent Recordings items, by position.
#[derive(Default)]
pub struct TrayState {
    recent: Mutex<Vec<String>>,
}

fn sdf_circle(px: f64, py: f64, cx: f64, cy: f64, r: f64) -> f64 {
    (px - cx).hypot(py - cy) - r
}

/// Template images (black with alpha): a record button, or a stop button while recording.
pub fn icon_pixels(recording: bool) -> Vec<u8> {
    let scale = ICON_PX as f64 / 18.0;
    let mut pixels = Vec::with_capacity(ICON_PX * ICON_PX * 4);
    for y in 0..ICON_PX {
        for x in 0..ICON_PX {
            let px = (x as f64 + 0.5) / scale;
            let py = (y as f64 + 0.5) / scale;
            let cover = |d: f64| (0.5 - d * scale).clamp(0.0, 1.0);
            let alpha = if recording {
                let disc = cover(sdf_circle(px, py, 9.0, 9.0, 8.0));
                let square = (px - 9.0).abs().max((py - 9.0).abs()) - 3.0;
                disc * (1.0 - cover(square))
            } else {
                let ring = cover((sdf_circle(px, py, 9.0, 9.0, 7.25)).abs() - 0.75);
                let dot = cover(sdf_circle(px, py, 9.0, 9.0, 4.0));
                ring.max(dot)
            };
            pixels.extend_from_slice(&[0, 0, 0, (alpha * 255.0).round() as u8]);
        }
    }
    pixels
}

fn icon(recording: bool) -> Image<'static> {
    Image::new_owned(icon_pixels(recording), ICON_PX as u32, ICON_PX as u32)
}

fn menu_id(action: ShortcutAction) -> &'static str {
    match action {
        ShortcutAction::RecordArea => "record-area",
        ShortcutAction::RecordWindow => "record-window",
        ShortcutAction::RecordDisplay => "record-display",
        ShortcutAction::CaptureArea => "capture-area",
        ShortcutAction::CaptureWindow => "capture-window",
        ShortcutAction::CaptureDisplay => "capture-display",
    }
}

fn menu_title(action: ShortcutAction) -> &'static str {
    match action {
        ShortcutAction::RecordArea => "Record Area",
        ShortcutAction::RecordWindow => "Record Window",
        ShortcutAction::RecordDisplay => "Record Display",
        ShortcutAction::CaptureArea => "Capture Area",
        ShortcutAction::CaptureWindow => "Capture Window",
        ShortcutAction::CaptureDisplay => "Capture Display",
    }
}

fn item(
    app: &AppHandle,
    id: &str,
    text: &str,
    enabled: bool,
    accelerator: Option<String>,
) -> tauri::Result<MenuItem<tauri::Wry>> {
    match MenuItem::with_id(app, id, text, enabled, accelerator.as_deref()) {
        Ok(item) => Ok(item),
        Err(e) => {
            log::warn!("menu item {id} without its shortcut: {e}");
            MenuItem::with_id(app, id, text, enabled, None::<&str>)
        }
    }
}

fn build_menu(app: &AppHandle, phase: &Phase) -> tauri::Result<Menu<tauri::Wry>> {
    let shortcut = |action| hotkeys::accelerator(app, action);
    let menu = Menu::new(app)?;
    let separator = || PredefinedMenuItem::separator(app);
    let idle = matches!(phase, Phase::Idle);

    match phase {
        Phase::Recording { .. } => {
            let stop_shortcut = ShortcutAction::ALL
                .into_iter()
                .filter(|a| a.records())
                .find_map(shortcut);
            menu.append(&item(app, "stop", "Stop Recording", true, stop_shortcut)?)?;
            menu.append(&item(app, "restart", "Restart Recording", true, None)?)?;
            menu.append(&item(app, "cancel", "Cancel Recording", true, None)?)?;
            menu.append(&separator()?)?;
        }
        Phase::Countdown | Phase::Starting => {
            let text = if matches!(phase, Phase::Countdown) {
                "Cancel Countdown"
            } else {
                "Cancel Recording"
            };
            menu.append(&item(app, "cancel", text, true, None)?)?;
            menu.append(&separator()?)?;
        }
        Phase::Stopping => {
            menu.append(&item(app, "saving", "Saving Recording…", false, None)?)?;
            menu.append(&separator()?)?;
        }
        Phase::Idle | Phase::Picking => {}
    }

    for group in [&ShortcutAction::ALL[..3], &ShortcutAction::ALL[3..]] {
        for &action in group {
            let enabled = idle || !action.records();
            menu.append(&item(
                app,
                menu_id(action),
                menu_title(action),
                enabled,
                shortcut(action),
            )?)?;
        }
        menu.append(&separator()?)?;
    }

    let root = app.state::<SettingsStore>().get().recording.dir();
    let recent = recent(editor::list_projects(&root), RECENT_COUNT);
    let recent_items: Vec<MenuItem<tauri::Wry>> = recent
        .iter()
        .enumerate()
        .map(|(i, r)| item(app, &format!("{RECENT_PREFIX}{i}"), &r.label, true, None))
        .collect::<tauri::Result<_>>()?;
    let empty = item(app, "no-recent", "No Recordings Yet", false, None)?;
    let refs: Vec<&dyn IsMenuItem<tauri::Wry>> = if recent_items.is_empty() {
        vec![&empty]
    } else {
        recent_items
            .iter()
            .map(|i| i as &dyn IsMenuItem<tauri::Wry>)
            .collect()
    };
    menu.append(&Submenu::with_items(app, "Recent Recordings", true, &refs)?)?;
    *app.state::<TrayState>()
        .recent
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = recent.into_iter().map(|r| r.path).collect();
    menu.append(&item(
        app,
        "open-folder",
        "Open Recordings Folder",
        true,
        None,
    )?)?;
    menu.append(&item(app, "library", "Library…", true, None)?)?;
    menu.append(&separator()?)?;
    menu.append(&item(
        app,
        "settings",
        "Settings…",
        true,
        Some("Cmd+Comma".into()),
    )?)?;
    menu.append(&item(app, "updates", "Check for Updates…", true, None)?)?;
    menu.append(&separator()?)?;
    menu.append(&item(
        app,
        "quit",
        "Quit Recast",
        true,
        Some("Cmd+KeyQ".into()),
    )?)?;
    Ok(menu)
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let phase = app.state::<Flow>().phase();
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon(false))
        .icon_as_template(true)
        .tooltip("Recast")
        .menu(&build_menu(app, &phase)?)
        .show_menu_on_left_click(true)
        .on_menu_event(on_menu)
        .on_tray_icon_event(on_tray)
        .build(app)?;
    Ok(())
}

/// Updates the icon and menu to the recording phase and the latest recordings.
pub fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let phase = app.state::<Flow>().phase();
    let active = matches!(
        phase,
        Phase::Recording { .. } | Phase::Countdown | Phase::Starting
    );
    if let Err(e) = tray
        .set_icon(Some(icon(active)))
        .and_then(|_| tray.set_icon_as_template(true))
        .and_then(|_| tray.set_show_menu_on_left_click(!active))
    {
        log::warn!("cannot update the menu bar icon: {e}");
    }
    match build_menu(app, &phase) {
        Ok(menu) => {
            if let Err(e) = tray.set_menu(Some(menu)) {
                log::warn!("cannot update the menu: {e}");
            }
        }
        Err(e) => log::warn!("cannot build the menu: {e}"),
    }
}

fn on_tray(tray: &TrayIcon, event: TrayIconEvent) {
    let app = tray.app_handle();
    match event {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } if matches!(
            app.state::<Flow>().phase(),
            Phase::Recording { .. } | Phase::Countdown | Phase::Starting
        ) =>
        {
            crate::actions::stop_recording(app);
        }
        TrayIconEvent::Enter { .. } if matches!(app.state::<Flow>().phase(), Phase::Idle) => {
            refresh(app);
        }
        _ => {}
    }
}

fn on_menu(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();
    match id {
        "stop" => crate::actions::stop_recording(app),
        "restart" => crate::actions::restart_recording(app),
        "cancel" => crate::actions::cancel_recording(app),
        "open-folder" => {
            let root = app.state::<SettingsStore>().get().recording.dir();
            crate::actions::open_folder(&root);
        }
        "library" => windows::show_or_log(app, windows::LIBRARY),
        "settings" => windows::show_or_log(app, windows::SETTINGS),
        "updates" => {
            tauri::async_runtime::spawn(crate::updates::check(app.clone(), true));
        }
        "quit" => app.exit(0),
        _ => {
            if let Some(action) = ShortcutAction::ALL.into_iter().find(|a| menu_id(*a) == id) {
                crate::actions::run(app, action);
            } else if let Some(index) = id
                .strip_prefix(RECENT_PREFIX)
                .and_then(|i| i.parse::<usize>().ok())
            {
                let path = app
                    .state::<TrayState>()
                    .recent
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(index)
                    .cloned();
                if let Some(path) = path
                    && let Err(e) = editor::commands::open_editor_window(app, Path::new(&path))
                {
                    log::warn!("cannot open {path}: {e}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str, created: f64, duration_ms: f64) -> ProjectSummary {
        ProjectSummary {
            path: format!("/m/{name}.recast"),
            name: name.into(),
            created_at_unix_ms: created,
            duration_ms,
            width: 100,
            height: 100,
        }
    }

    #[test]
    fn recent_lists_the_newest_five() {
        let projects = (0..8)
            .map(|i| project(&format!("Take {i}"), i as f64 * 1000.0, 65_400.0))
            .collect();
        let recent = recent(projects, RECENT_COUNT);
        assert_eq!(recent.len(), 5);
        assert_eq!(recent[0].path, "/m/Take 7.recast");
        assert_eq!(recent[0].label, "Take 7  (1:05)");
        assert_eq!(recent[4].path, "/m/Take 3.recast");
    }

    #[test]
    fn recent_handles_few_or_no_recordings() {
        assert!(recent(Vec::new(), RECENT_COUNT).is_empty());
        let one = recent(vec![project("Only", 1.0, 400.0)], RECENT_COUNT);
        assert_eq!(one[0].label, "Only  (0:00)");
    }

    #[test]
    fn recent_reads_finished_bundles_only() {
        let dir = tempfile::tempdir().unwrap();
        for (name, created) in [("Older", 1_000.0), ("Newer", 2_000.0), ("Crashed", 3_000.0)] {
            let path = dir.path().join(format!("{name}.recast"));
            std::fs::create_dir(&path).unwrap();
            std::fs::write(
                path.join(recast_project::PROJECT_FILE),
                serde_json::to_vec(&editor::test_support::project(name, created)).unwrap(),
            )
            .unwrap();
        }
        std::fs::write(
            dir.path()
                .join("Crashed.recast")
                .join(recast_project::MARKER_FILE),
            b"{}",
        )
        .unwrap();

        let recent = recent(editor::list_projects(dir.path()), RECENT_COUNT);
        let names: Vec<_> = recent
            .iter()
            .map(|r| r.label.split("  ").next().unwrap())
            .collect();
        assert_eq!(names, ["Newer", "Older"]);
        assert!(recent[0].path.ends_with("Newer.recast"));
    }

    #[test]
    fn every_action_has_a_menu_item() {
        let ids: Vec<_> = ShortcutAction::ALL.map(menu_id).into();
        assert_eq!(
            ids,
            [
                "record-area",
                "record-window",
                "record-display",
                "capture-area",
                "capture-window",
                "capture-display"
            ]
        );
        let titles: Vec<_> = ShortcutAction::ALL.map(menu_title).into();
        assert_eq!(
            titles,
            [
                "Record Area",
                "Record Window",
                "Record Display",
                "Capture Area",
                "Capture Window",
                "Capture Display"
            ]
        );
    }

    #[test]
    fn icons_differ_while_recording() {
        let idle = icon_pixels(false);
        let recording = icon_pixels(true);
        assert_eq!(idle.len(), ICON_PX * ICON_PX * 4);
        let alpha = |pixels: &[u8], x: usize, y: usize| pixels[(y * ICON_PX + x) * 4 + 3];
        let c = ICON_PX / 2;
        assert_eq!(alpha(&idle, c, c), 255, "record dot");
        assert_eq!(alpha(&recording, c, c), 0, "stop square is cut out");
        assert_eq!(alpha(&recording, c, 4), 255, "disc around it");
        assert_eq!(alpha(&idle, 0, 0), 0);
        assert!(
            idle.chunks(4).all(|p| p[..3] == [0, 0, 0]),
            "template images are black"
        );
    }
}
