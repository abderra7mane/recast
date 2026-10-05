use cidre::{arc, cg, ns, sc};

use super::{block_on, writer::platform};
use crate::{DisplayInfo, Error, Rect, Result, WindowInfo};

pub(super) fn shareable_content() -> Result<arc::R<sc::ShareableContent>> {
    block_on(sc::ShareableContent::current()).map_err(|e| {
        if cg::screen_capture_access::preflight() {
            platform("cannot list screen content", &e)
        } else {
            Error::PermissionDenied
        }
    })
}

pub(super) fn rect(r: cg::Rect) -> Rect {
    Rect {
        x: r.origin.x,
        y: r.origin.y,
        width: r.size.width,
        height: r.size.height,
    }
}

/// The running app entry for this process, so its windows can be excluded from capture.
pub(super) fn own_apps(content: &sc::ShareableContent) -> arc::R<ns::Array<sc::RunningApp>> {
    let pid = std::process::id() as i32;
    let apps: Vec<arc::R<sc::RunningApp>> = content
        .apps()
        .iter()
        .filter(|app| app.process_id() == pid)
        .map(|app| app.retained())
        .collect();
    ns::Array::from_slice_retained(&apps)
}

pub(super) fn display_scale(display: &sc::Display) -> f64 {
    let filter = sc::ContentFilter::with_display_excluding_windows(display, &ns::Array::new());
    sc::ShareableContent::info_for_filter(&filter).point_pixel_scale() as f64
}

pub(super) fn find_display(content: &sc::ShareableContent, id: u32) -> Result<arc::R<sc::Display>> {
    content
        .displays()
        .iter()
        .find(|d| d.display_id().0 == id)
        .map(|d| d.retained())
        .ok_or_else(|| Error::NotFound(format!("display {id}")))
}

pub(super) fn find_window(content: &sc::ShareableContent, id: u32) -> Result<arc::R<sc::Window>> {
    content
        .windows()
        .iter()
        .find(|w| w.id() == id)
        .map(|w| w.retained())
        .ok_or_else(|| Error::NotFound(format!("window {id}")))
}

pub(super) fn displays() -> Result<Vec<DisplayInfo>> {
    let content = shareable_content()?;
    let main = cg::DirectDisplayId::main().0;
    let mut out = Vec::new();
    for (index, display) in content.displays().iter().enumerate() {
        let id = display.display_id().0;
        let frame = display.frame();
        let name = if id == main {
            "Main display".to_string()
        } else {
            format!("Display {}", index + 1)
        };
        out.push(DisplayInfo {
            id,
            name,
            bounds: rect(frame),
            scale_factor: display_scale(display),
        });
    }
    Ok(out)
}

pub(super) fn windows() -> Result<Vec<WindowInfo>> {
    let content = shareable_content()?;
    let pid = std::process::id() as i32;
    let mut out = Vec::new();
    for window in content.windows().iter() {
        let frame = window.frame();
        if !window.is_on_screen()
            || window.window_layer() != 0
            || frame.size.width < 40.0
            || frame.size.height < 40.0
        {
            continue;
        }
        let app = window.owning_app();
        if app.as_ref().is_some_and(|a| a.process_id() == pid) {
            continue;
        }
        let app_name = app.map(|a| a.app_name().to_string()).unwrap_or_default();
        let title = window.title().map(|t| t.to_string()).unwrap_or_default();
        if title.is_empty() && app_name.is_empty() {
            continue;
        }
        out.push(WindowInfo {
            id: window.id(),
            title,
            app_name,
            bounds: rect(frame),
        });
    }
    Ok(out)
}
