//! The Library, Settings and Onboarding windows; one of each at most.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LIBRARY: &str = "library";
pub const SETTINGS: &str = "settings";
pub const ONBOARDING: &str = "onboarding";

struct Spec {
    title: &'static str,
    size: (f64, f64),
    min_size: (f64, f64),
    resizable: bool,
}

fn spec(label: &str) -> Spec {
    match label {
        SETTINGS => Spec {
            title: "Recast Settings",
            size: (640.0, 560.0),
            min_size: (560.0, 460.0),
            resizable: true,
        },
        ONBOARDING => Spec {
            title: "Welcome to Recast",
            size: (560.0, 600.0),
            min_size: (560.0, 600.0),
            resizable: false,
        },
        _ => Spec {
            title: "Recast Library",
            size: (560.0, 720.0),
            min_size: (480.0, 480.0),
            resizable: true,
        },
    }
}

/// Shows the window with `label`, creating it when needed.
pub fn show(app: &AppHandle, label: &str) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.unminimize();
        let _ = window.show();
        return window.set_focus().map_err(|e| e.to_string());
    }
    let spec = spec(label);
    crate::activation::build(
        WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
            .title(spec.title)
            .inner_size(spec.size.0, spec.size.1)
            .min_inner_size(spec.min_size.0, spec.min_size.1)
            .resizable(spec.resizable),
    )?;
    Ok(())
}

pub fn show_or_log(app: &AppHandle, label: &str) {
    if let Err(e) = show(app, label) {
        log::warn!("cannot open the {label} window: {e}");
    }
}
