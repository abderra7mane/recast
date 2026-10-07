//! The target picker shared by screenshots and recording.

pub mod cursor;
mod overlay;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use objc2::MainThreadMarker;
use recast_capture::{
    ScreenCapture,
    picker::{PickMode, Picked, Picker, PickerDisplay, PickerWindow},
};
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::focus::Focus;

/// What the picked target is for; it only changes the hint's wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Record,
    Capture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickRequest {
    pub mode: PickMode,
    pub purpose: Purpose,
}

impl PickRequest {
    pub fn new(mode: PickMode, purpose: Purpose) -> Self {
        Self { mode, purpose }
    }

    /// The picker behind the overlays; the same for recording and screenshots.
    pub fn picker(
        &self,
        displays: Vec<PickerDisplay>,
        windows: Vec<PickerWindow>,
        own_pid: i32,
    ) -> Picker {
        Picker::new(self.mode, displays, windows, own_pid)
    }

    /// The hint shown at the top of the display under the pointer.
    pub fn hint(&self) -> String {
        let verb = match self.purpose {
            Purpose::Record => "record",
            Purpose::Capture => "capture",
        };
        let what = match self.mode {
            PickMode::Area => format!("Drag to select an area to {verb}"),
            PickMode::Window => format!("Click a window to {verb} it"),
            PickMode::Display => format!("Click a display to {verb} it"),
        };
        format!("{what} · Esc to cancel")
    }
}

static OPEN: AtomicBool = AtomicBool::new(false);
/// Numbers each picker, so a window list that arrives late reaches the picker that asked
/// for it.
static GENERATION: AtomicU64 = AtomicU64::new(0);

struct OpenGuard;

impl Drop for OpenGuard {
    fn drop(&mut self) {
        OPEN.store(false, Ordering::SeqCst);
    }
}

pub struct Pick {
    pub picked: Picked,
    /// Activation taken from the app the user was in; give it back once the overlays
    /// that follow the pick are gone.
    pub focus: Focus,
}

/// Lets the user pick a display, window or region; `None` when they cancel.
pub async fn pick(app: &AppHandle, request: PickRequest) -> Result<Option<Pick>, String> {
    if OPEN.swap(true, Ordering::SeqCst) {
        return Err("The picker is already open.".into());
    }
    let _guard = OpenGuard;
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    // A drag that began before the overlays could show, as one can while a menu bar menu
    // closes, belongs to the app below; the overlays wait for it to end.
    tauri::async_runtime::spawn_blocking(|| {
        while crate::appkit::left_button_down() {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let own_pid = std::process::id() as i32;
    app.run_on_main_thread(move || {
        let mtm = MainThreadMarker::new().expect("runs on the main thread");
        let focus = Focus::take(mtm);
        overlay::open(
            mtm,
            request,
            generation,
            own_pid,
            Box::new(move |picked| {
                let pick = match picked {
                    Some(picked) => Some(Pick { picked, focus }),
                    None => {
                        focus.restore(mtm);
                        None
                    }
                };
                let _ = tx.send(pick);
            }),
        );
    })
    .map_err(|e| e.to_string())?;
    // Listing the windows is slow, and a click before the overlays show goes to the app
    // below, so Window mode loads them while its overlays are already up.
    if request.mode == PickMode::Window {
        load_windows(app.clone(), generation);
    }
    rx.await
        .map_err(|_| "The picker closed unexpectedly.".to_string())
}

fn load_windows(app: AppHandle, generation: u64) {
    tauri::async_runtime::spawn(async move {
        let listed =
            tauri::async_runtime::spawn_blocking(|| recast_capture::platform().window_stack())
                .await;
        match listed {
            Ok(Ok(windows)) => {
                let shown = app.run_on_main_thread(move || {
                    overlay::set_windows(generation, windows);
                });
                if let Err(e) = shown {
                    log::warn!("cannot reach the main thread: {e}");
                }
            }
            Ok(Err(e)) => log::warn!("cannot list the windows to pick from: {e}"),
            Err(e) => log::warn!("cannot list the windows to pick from: {e}"),
        }
    });
}

/// Hides the Library window so it doesn't cover what the user is picking.
pub fn hide_library(app: &AppHandle) -> Option<WebviewWindow> {
    let library = app
        .get_webview_window(crate::windows::LIBRARY)
        .filter(|w| w.is_visible().unwrap_or(false))?;
    let _ = library.hide();
    Some(library)
}

pub fn show_library(library: Option<WebviewWindow>) {
    if let Some(library) = library {
        let _ = library.show();
    }
}

#[cfg(test)]
mod tests {
    use recast_capture::{
        CaptureTarget, Rect,
        picker::{Highlight, Outcome, Point},
    };

    use super::*;

    fn displays() -> Vec<PickerDisplay> {
        let display = |id, x, width, height| PickerDisplay {
            id,
            name: format!("Display {id}"),
            bounds: Rect {
                x,
                y: 0.0,
                width,
                height,
            },
            scale_factor: 2.0,
        };
        vec![
            display(1, 0.0, 1512.0, 982.0),
            display(2, 1512.0, 1920.0, 1080.0),
        ]
    }

    fn windows() -> Vec<PickerWindow> {
        vec![PickerWindow {
            id: 7,
            pid: 3,
            layer: 0,
            app_name: "Notes".into(),
            title: String::new(),
            bounds: Rect {
                x: 1600.0,
                y: 100.0,
                width: 800.0,
                height: 600.0,
            },
        }]
    }

    #[test]
    fn record_display_and_capture_display_pick_alike() {
        let record = PickRequest::new(PickMode::Display, Purpose::Record);
        let capture = PickRequest::new(PickMode::Display, Purpose::Capture);
        for at in [
            Point::new(300.0, 200.0),
            Point::new(1700.0, 200.0),
            Point::new(3000.0, 1000.0),
            Point::new(100.0, 1000.0),
        ] {
            let mut pickers = [record, capture].map(|r| r.picker(displays(), windows(), 99));
            let results = pickers.each_mut().map(|picker| {
                picker.move_to(at);
                let highlight = picker.highlight();
                let cursor = picker.cursor();
                picker.press(at);
                (highlight, cursor, picker.release(at))
            });
            assert_eq!(results[0], results[1], "{at:?}");
        }

        let mut picker = record.picker(displays(), windows(), 99);
        picker.move_to(Point::new(1700.0, 200.0));
        assert!(matches!(
            picker.highlight(),
            Highlight::Display { ref label, .. } if label == "Display 2  1920 × 1080"
        ));
        picker.press(Point::new(1700.0, 200.0));
        let Outcome::Picked(picked) = picker.release(Point::new(1700.0, 200.0)) else {
            panic!("expected display 2");
        };
        assert_eq!(picked.target, CaptureTarget::Display { display_id: 2 });
    }

    #[test]
    fn hints_name_the_mode_and_purpose() {
        let hint = |mode, purpose| PickRequest::new(mode, purpose).hint();
        assert_eq!(
            hint(PickMode::Area, Purpose::Capture),
            "Drag to select an area to capture · Esc to cancel"
        );
        assert_eq!(
            hint(PickMode::Area, Purpose::Record),
            "Drag to select an area to record · Esc to cancel"
        );
        assert_eq!(
            hint(PickMode::Window, Purpose::Record),
            "Click a window to record it · Esc to cancel"
        );
        assert_eq!(
            hint(PickMode::Display, Purpose::Capture),
            "Click a display to capture it · Esc to cancel"
        );
    }
}
