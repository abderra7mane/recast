//! Runs the recording flow for the menu, the hotkey, the Library and the control bar:
//! pick a target, count down, record, then stop, restart or cancel.

pub mod bar_layout;
mod control_bar;
mod countdown;

use std::{
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};

use objc2::MainThreadMarker;
use recast_capture::{CaptureTarget, picker::PickMode};
use recast_project::Rect;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use self::bar_layout::BarButton;
use crate::{
    alert,
    editor::commands::open_editor_window,
    flow::{Flow, Next, Phase, Stopped},
    focus::Focus,
    picker,
    recording::{RecordingRequest, RecordingStatus, Session, default_name},
    settings::SettingsStore,
};

pub const COUNTDOWN_SECONDS: u32 = 3;
/// Longest wait at quit for a running recording to be saved.
const QUIT_SAVE_WAIT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct RecordingChanged {
    pub phase: Phase,
}

#[derive(Debug, Clone, Copy)]
struct Placement {
    display_id: u32,
    /// The recorded window or area, outlined during the countdown.
    outline: Option<Rect>,
}

#[derive(Default)]
pub struct RecorderUi {
    placement: Mutex<Option<Placement>>,
    /// Activation taken from the app the user was in, given back when recording starts.
    focus: Mutex<Option<Focus>>,
}

impl RecorderUi {
    fn placement(&self) -> Option<Placement> {
        *self.placement.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn take_focus(&self) -> Option<Focus> {
        self.focus.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}

/// Tells the windows and the menu bar icon that the phase changed.
pub fn changed(app: &AppHandle) {
    let phase = app.state::<Flow>().phase();
    let _ = RecordingChanged { phase }.emit(app);
    crate::tray::refresh(app);
}

fn starter(app: &AppHandle) -> impl Fn(&RecordingRequest) -> Result<Session, String> + use<> {
    let root = app.state::<SettingsStore>().get().recording.dir();
    move |request| {
        Session::start(
            &root,
            &default_name(),
            request,
            &recast_capture::platform(),
            &recast_input::platform(),
        )
    }
}

async fn blocking<T: Send + 'static>(
    app: &AppHandle,
    work: impl FnOnce(&AppHandle) -> T + Send + 'static,
) -> Result<T, String> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || work(&app))
        .await
        .map_err(|e| e.to_string())
}

fn on_main(app: &AppHandle, work: impl FnOnce(MainThreadMarker) + Send + 'static) {
    if let Err(e) = app.run_on_main_thread(move || {
        work(MainThreadMarker::new().expect("runs on the main thread"));
    }) {
        log::warn!("cannot reach the main thread: {e}");
    }
}

fn restore_focus(app: &AppHandle) {
    if let Some(focus) = app.state::<RecorderUi>().take_focus() {
        focus.restore_from(app);
    }
}

fn close_overlays(app: &AppHandle) {
    on_main(app, |mtm| {
        countdown::close(mtm);
        control_bar::close(mtm);
    });
}

/// Picks a target with `mode` and starts the countdown, or records right away when it
/// is off.
pub async fn start(app: AppHandle, mode: PickMode) -> Result<(), String> {
    let flow = app.state::<Flow>();
    flow.begin_pick()?;
    changed(&app);
    let library = picker::hide_library(&app);
    let picked = picker::pick(
        &app,
        picker::PickRequest::new(mode, picker::Purpose::Record),
    )
    .await;
    picker::show_library(library);
    let pick = match picked {
        Ok(Some(pick)) => pick,
        other => {
            flow.pick_cancelled();
            changed(&app);
            return other.map(|_| ());
        }
    };

    let settings = app.state::<SettingsStore>().get().recording;
    let picked = pick.picked;
    let outline =
        (!matches!(picked.target, CaptureTarget::Display { .. })).then_some(picked.bounds);
    let ui = app.state::<RecorderUi>();
    *ui.placement.lock().unwrap_or_else(|e| e.into_inner()) = Some(Placement {
        display_id: picked.display_id,
        outline,
    });
    if settings.countdown {
        *ui.focus.lock().unwrap_or_else(|e| e.into_inner()) = Some(pick.focus);
    } else {
        pick.focus.restore_from(&app);
    }
    let request = RecordingRequest {
        target: picked.target,
        system_audio: settings.system_audio,
        mic: settings.mic,
    };
    let countdown = settings.countdown;
    let next = blocking(&app, move |app| {
        app.state::<Flow>()
            .picked(request, countdown, &starter(app))
    })
    .await?;
    proceed(&app, next)
}

fn proceed(app: &AppHandle, next: Result<Next, String>) -> Result<(), String> {
    let result = match next {
        Ok(Next::Countdown(id)) => {
            show_countdown(app, id);
            Ok(())
        }
        Ok(Next::Recording(status)) => {
            show_bar(app, &status);
            Ok(())
        }
        Ok(Next::Cancelled) => Ok(()),
        Err(e) => {
            restore_focus(app);
            Err(e)
        }
    };
    changed(app);
    result
}

fn show_countdown(app: &AppHandle, id: u64) {
    let Some(placement) = app.state::<RecorderUi>().placement() else {
        return;
    };
    let handle = app.clone();
    on_main(app, move |mtm| {
        control_bar::close(mtm);
        {
            let ui = handle.state::<RecorderUi>();
            let mut focus = ui.focus.lock().unwrap_or_else(|e| e.into_inner());
            if focus.is_none() {
                *focus = Some(Focus::take(mtm));
            }
        }
        let done_app = handle.clone();
        countdown::show(
            mtm,
            placement.display_id,
            placement.outline,
            COUNTDOWN_SECONDS,
            Box::new(move || {
                if let Some(focus) = done_app.state::<RecorderUi>().take_focus() {
                    focus.restore(mtm);
                }
                tauri::async_runtime::spawn(countdown_done(done_app, id));
            }),
        );
    });
}

async fn countdown_done(app: AppHandle, id: u64) {
    let result = blocking(&app, move |app| {
        app.state::<Flow>().countdown_done(id, &starter(app))
    })
    .await
    .and_then(|r| r);
    match result {
        Ok(Some(status)) => show_bar(&app, &status),
        Ok(None) => {}
        Err(e) => alert::error(&app, format!("Recording didn't start: {e}")),
    }
    changed(&app);
}

fn show_bar(app: &AppHandle, status: &RecordingStatus) {
    if !status.input_events {
        log::warn!("Input Monitoring is off: clicks are not recorded");
    }
    let Some(placement) = app.state::<RecorderUi>().placement() else {
        return;
    };
    let elapsed = Duration::from_secs_f64(status.elapsed_ms.max(0.0) / 1000.0);
    let started = Instant::now()
        .checked_sub(elapsed)
        .unwrap_or_else(Instant::now);
    let handle = app.clone();
    on_main(app, move |mtm| {
        countdown::close(mtm);
        // A Stop or Cancel while the recording was starting has ended it already.
        if !handle.state::<Flow>().is_recording() {
            return;
        }
        control_bar::show(
            mtm,
            placement.display_id,
            started,
            Box::new(move |button| match button {
                BarButton::Stop => crate::actions::stop_recording(&handle),
                BarButton::Restart => crate::actions::restart_recording(&handle),
                BarButton::Cancel => crate::actions::cancel_recording(&handle),
            }),
        );
    });
}

/// Saves the recording and opens it in the editor, or cancels the countdown.
pub async fn stop(app: AppHandle) {
    close_overlays(&app);
    let result = blocking(&app, |app| app.state::<Flow>().stop())
        .await
        .and_then(|r| r);
    // Also removes a bar shown by a start that finished while this stop waited.
    close_overlays(&app);
    restore_focus(&app);
    match result {
        Ok(Stopped::Finished(finished)) => {
            for warning in &finished.warnings {
                log::warn!("{}: {warning}", finished.bundle_path);
            }
            if let Err(e) = open_editor_window(&app, Path::new(&finished.bundle_path)) {
                alert::error(
                    &app,
                    format!("The recording was saved but can't be opened: {e}"),
                );
            }
        }
        Ok(Stopped::Cancelled | Stopped::Nothing) => {}
        Err(e) => alert::error(&app, format!("The recording couldn't be saved: {e}")),
    }
    changed(&app);
}

/// Deletes the recording and records the same target again.
pub async fn restart(app: AppHandle) {
    close_overlays(&app);
    let countdown = app.state::<SettingsStore>().get().recording.countdown;
    let next = blocking(&app, move |app| {
        app.state::<Flow>().restart(countdown, &starter(app))
    })
    .await
    .and_then(|r| r);
    match next {
        Ok(Some(next)) => {
            if let Err(e) = proceed(&app, Ok(next)) {
                alert::error(&app, format!("Recording didn't restart: {e}"));
            }
        }
        Ok(None) => changed(&app),
        Err(e) => {
            alert::error(&app, format!("Recording didn't restart: {e}"));
            changed(&app);
        }
    }
}

/// Deletes the recording, or ends the countdown.
pub async fn cancel(app: AppHandle) {
    close_overlays(&app);
    if let Err(e) = blocking(&app, |app| app.state::<Flow>().cancel()).await {
        log::warn!("cannot cancel the recording: {e}");
    }
    close_overlays(&app);
    restore_focus(&app);
    changed(&app);
}

/// Starts recording with `mode` when idle; stops the recording or countdown otherwise.
pub async fn toggle(app: AppHandle, mode: PickMode) {
    match app.state::<Flow>().phase() {
        Phase::Idle => {
            if let Err(e) = start(app.clone(), mode).await {
                alert::error(&app, e);
            }
        }
        Phase::Recording { .. } | Phase::Countdown | Phase::Starting => stop(app).await,
        Phase::Picking | Phase::Stopping => {}
    }
}

/// At quit, saves a running recording instead of leaving it for recovery, or waits for
/// one being saved.
pub fn save_on_quit(app: &AppHandle) {
    let flow = app.state::<Flow>();
    if !flow.wait_while_stopping(QUIT_SAVE_WAIT) {
        log::warn!("saving the recording took too long at quit");
        return;
    }
    if !flow.is_recording() {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = app.clone();
    std::thread::spawn(move || {
        let _ = tx.send(handle.state::<Flow>().stop());
    });
    match rx.recv_timeout(QUIT_SAVE_WAIT) {
        Ok(Ok(_)) => log::info!("saved the running recording at quit"),
        Ok(Err(e)) => log::warn!("cannot save the running recording at quit: {e}"),
        Err(_) => log::warn!("saving the running recording took too long at quit"),
    }
}
