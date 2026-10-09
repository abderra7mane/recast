use std::collections::HashMap;

use cidre::{arc, cg, ns, sc};

use super::{block_on, writer::platform};
use crate::{
    CaptureTarget, DisplayInfo, Error, Rect, Result, WindowInfo, check_region, picker::PickerWindow,
};

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

/// On-screen windows that ScreenCaptureKit can capture, ordered front to back.
pub(super) fn window_stack() -> Result<Vec<PickerWindow>> {
    let content = shareable_content()?;
    let windows = content.windows();
    let by_id: HashMap<u32, &sc::Window> = windows.iter().map(|w| (w.id(), w)).collect();
    let order = cg::WindowList::new(
        cg::WindowListOpt::ON_SCREEN_ONLY | cg::WindowListOpt::EXCLUDE_DESKTOP_ELEMENTS,
        cg::WINDOW_ID_NULL,
    )
    .ok_or_else(|| Error::Platform("cannot list the windows on screen".into()))?;
    let mut out = Vec::new();
    for index in 0..order.len() {
        let Some(window) = by_id.get(&order.get(index)) else {
            continue;
        };
        let app = window.owning_app();
        out.push(PickerWindow {
            id: window.id(),
            pid: app.as_ref().map_or(-1, |a| a.process_id()),
            layer: window.window_layer() as i32,
            app_name: app.map(|a| a.app_name().to_string()).unwrap_or_default(),
            title: window.title().map(|t| t.to_string()).unwrap_or_default(),
            bounds: rect(window.frame()),
        });
    }
    Ok(out)
}

/// What a capture target resolves to in ScreenCaptureKit.
pub(super) struct Source {
    pub filter: arc::R<sc::ContentFilter>,
    pub scale: f64,
    /// Captured size in points.
    pub width: f64,
    pub height: f64,
    /// Captured area in global display points.
    pub bounds: Rect,
    /// Part of the display to capture, in display points.
    pub src_rect: Option<cg::Rect>,
    pub window: Option<arc::R<sc::Window>>,
}

pub(super) fn source(content: &sc::ShareableContent, target: &CaptureTarget) -> Result<Source> {
    let display_filter = |display: &sc::Display| {
        sc::ContentFilter::with_display_excluding_windows(display, &ns::Array::new())
    };
    let scale = |filter: &sc::ContentFilter| {
        sc::ShareableContent::info_for_filter(filter).point_pixel_scale() as f64
    };
    match target {
        CaptureTarget::Display { display_id } => {
            let display = find_display(content, *display_id)?;
            let filter = display_filter(&display);
            let bounds = rect(display.frame());
            Ok(Source {
                scale: scale(&filter),
                filter,
                width: bounds.width,
                height: bounds.height,
                bounds,
                src_rect: None,
                window: None,
            })
        }
        CaptureTarget::Region {
            display_id,
            rect: region,
        } => {
            let display = find_display(content, *display_id)?;
            let frame = rect(display.frame());
            check_region(&frame, region)?;
            let filter = display_filter(&display);
            Ok(Source {
                scale: scale(&filter),
                filter,
                width: region.width,
                height: region.height,
                bounds: Rect {
                    x: frame.x + region.x,
                    y: frame.y + region.y,
                    ..*region
                },
                src_rect: Some(cg::Rect {
                    origin: cg::Point {
                        x: region.x,
                        y: region.y,
                    },
                    size: cg::Size {
                        width: region.width,
                        height: region.height,
                    },
                }),
                window: None,
            })
        }
        CaptureTarget::Window { window_id } => {
            let window = find_window(content, *window_id)?;
            let filter = sc::ContentFilter::with_desktop_independent_window(&window);
            let info = sc::ShareableContent::info_for_filter(&filter);
            let size = info.content_rect().size;
            Ok(Source {
                scale: info.point_pixel_scale() as f64,
                filter,
                width: size.width,
                height: size.height,
                bounds: rect(window.frame()),
                src_rect: None,
                window: Some(window),
            })
        }
    }
}
