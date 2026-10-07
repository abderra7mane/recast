//! Makes Recast the active app while its picker or countdown is up over another app, and
//! later hands activation back to the app the user was in.

use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSRunningApplication, NSWorkspace,
};
use tauri::AppHandle;

#[derive(Debug, Default)]
pub struct Focus {
    previous: Option<i32>,
}

impl Focus {
    /// Activates Recast unless it is active already.
    pub fn take(mtm: MainThreadMarker) -> Self {
        let app = NSApplication::sharedApplication(mtm);
        if app.isActive() {
            return Self::default();
        }
        let own = std::process::id() as i32;
        let previous = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|a| a.processIdentifier())
            .filter(|pid| *pid != own && *pid > 0);
        app.activate();
        Self { previous }
    }

    /// Reactivates the app that was active before, unless the user has moved on.
    pub fn restore(self, mtm: MainThreadMarker) {
        let Some(pid) = self.previous else {
            return;
        };
        if !NSApplication::sharedApplication(mtm).isActive() {
            return;
        }
        if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
            app.activateWithOptions(NSApplicationActivationOptions::empty());
        }
    }

    pub fn restore_from(self, app: &AppHandle) {
        let _ = app.run_on_main_thread(move || {
            self.restore(MainThreadMarker::new().expect("runs on the main thread"));
        });
    }
}
