//! Recast lives in the menu bar. It shows in the Dock and the app switcher only while
//! one of its regular windows is open.

use std::{collections::BTreeSet, sync::Mutex};

use tauri::{ActivationPolicy, AppHandle, Manager, WebviewWindow, WebviewWindowBuilder, Wry};

const REGULAR_LABELS: &[&str] = &["library", "settings", "onboarding"];
const REGULAR_PREFIXES: &[&str] = &[
    crate::editor::commands::LABEL_PREFIX,
    crate::screenshots::markup::LABEL_PREFIX,
];

fn is_regular(label: &str) -> bool {
    REGULAR_LABELS.contains(&label) || REGULAR_PREFIXES.iter().any(|p| label.starts_with(p))
}

/// The open regular windows, and the policy they call for.
#[derive(Default)]
pub struct Tracker {
    open: Mutex<BTreeSet<String>>,
}

impl Tracker {
    fn policy(open: &BTreeSet<String>) -> ActivationPolicy {
        if open.is_empty() {
            ActivationPolicy::Accessory
        } else {
            ActivationPolicy::Regular
        }
    }

    /// Records a window; returns the policy to switch to when it changes.
    pub fn opened(&self, label: &str) -> Option<ActivationPolicy> {
        if !is_regular(label) {
            return None;
        }
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        let was_empty = open.is_empty();
        open.insert(label.to_owned());
        was_empty.then(|| Self::policy(&open))
    }

    pub fn closed(&self, label: &str) -> Option<ActivationPolicy> {
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        (open.remove(label) && open.is_empty()).then(|| Self::policy(&open))
    }
}

fn apply(app: &AppHandle, policy: Option<ActivationPolicy>) {
    if let Some(policy) = policy
        && let Err(e) = app.set_activation_policy(policy)
    {
        log::warn!("cannot change the activation policy: {e}");
    }
}

/// Builds a window and shows Recast in the Dock until the window is destroyed. Every
/// window is built through here.
pub fn build<M: Manager<Wry>>(
    builder: WebviewWindowBuilder<'_, Wry, M>,
) -> Result<WebviewWindow, String> {
    let window = builder.build().map_err(|e| e.to_string())?;
    track(&window);
    Ok(window)
}

fn track(window: &WebviewWindow) {
    let app = window.app_handle();
    apply(app, app.state::<Tracker>().opened(window.label()));
    let _ = window.set_focus();
}

pub fn window_destroyed(app: &AppHandle, label: &str) {
    apply(app, app.state::<Tracker>().closed(label));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is(policy: Option<ActivationPolicy>, expected: ActivationPolicy) -> bool {
        matches!(
            (policy, expected),
            (Some(ActivationPolicy::Regular), ActivationPolicy::Regular)
                | (
                    Some(ActivationPolicy::Accessory),
                    ActivationPolicy::Accessory
                )
        )
    }

    #[test]
    fn regular_while_any_window_is_open() {
        let tracker = Tracker::default();
        assert!(is(tracker.opened("editor-0"), ActivationPolicy::Regular));
        assert!(tracker.opened("settings").is_none(), "already regular");
        assert!(tracker.opened("editor-0").is_none(), "counted once");
        assert!(tracker.closed("editor-0").is_none(), "settings still open");
        assert!(is(tracker.closed("settings"), ActivationPolicy::Accessory));
        assert!(tracker.closed("settings").is_none(), "already closed");
        assert!(is(tracker.opened("markup-3"), ActivationPolicy::Regular));
    }

    #[test]
    fn every_regular_window_counts() {
        for label in ["library", "settings", "onboarding", "editor-12", "markup-1"] {
            let tracker = Tracker::default();
            assert!(
                is(tracker.opened(label), ActivationPolicy::Regular),
                "{label}"
            );
            assert!(
                is(tracker.closed(label), ActivationPolicy::Accessory),
                "{label}"
            );
        }
    }

    #[test]
    fn every_window_is_built_through_build() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = vec![src];
        let mut checked = 0;
        while let Some(path) = files.pop() {
            if path.is_dir() {
                files.extend(std::fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") || path.ends_with("activation.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let builders = text.matches("WebviewWindowBuilder::new(").count();
            let tracked = text.matches("activation::build(").count();
            assert_eq!(
                builders,
                tracked,
                "{} builds a window without activation::build",
                path.display()
            );
            checked += builders;
        }
        assert_eq!(checked, 3, "library/settings/onboarding, editor and markup");
    }

    #[test]
    fn other_windows_do_not_count() {
        let tracker = Tracker::default();
        assert!(tracker.opened("main").is_none());
        assert!(tracker.opened("editor").is_none());
        assert!(tracker.closed("main").is_none());
    }
}
