//! Markup windows: annotate, crop and beautify a screenshot. Each window has a label
//! `markup-N`. The webview draws the markup and sends the flattened image back; Beautify
//! is applied here by the recording compositor.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use base64::Engine;
use recast_project::{BackgroundFill, BackgroundSettings};
use recast_render::{bitmap, still::StillRenderer};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{
    AppHandle, Manager, State, WebviewWindow, Wry,
    ipc::{Invoke, InvokeBody, Request, Response},
};

use super::{Capture, copy_png};
use crate::settings::SettingsStore;

pub const LABEL_PREFIX: &str = "markup-";
/// Largest output side; the GPU's texture limit on Apple Silicon.
const MAX_SIDE: u32 = 16_384;
const WIDTH_HEADER: &str = "x-recast-width";
const HEIGHT_HEADER: &str = "x-recast-height";
const TOKEN_HEADER: &str = "x-recast-token";

/// Straight RGBA, tightly packed.
#[derive(Debug, Clone, PartialEq)]
struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Image {
    /// Bytes of a `width × height` image, when both sides are between 1 and [`MAX_SIDE`].
    fn byte_len(width: u32, height: u32) -> Result<usize, String> {
        let side = 1..=MAX_SIDE;
        if !side.contains(&width) || !side.contains(&height) {
            return Err(format!("{width}×{height} is not a supported image size"));
        }
        (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| format!("{width}×{height} is too large"))
    }

    fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, String> {
        if rgba.len() != Self::byte_len(width, height)? {
            return Err(format!(
                "{} bytes is not a {width}×{height} RGBA image",
                rgba.len()
            ));
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    fn clear(width: u32, height: u32) -> Result<Self, String> {
        Self::new(width, height, vec![0; Self::byte_len(width, height)?])
    }
}

/// What the compositor holds: a clear image of a size, for frame previews, or an output.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Holds {
    Clear(u32, u32),
    Output,
}

struct Renderer {
    still: StillRenderer,
    holds: Holds,
}

impl Renderer {
    /// The renderer in `slot` holding `holds`; `image` makes the image when it must change.
    fn prepare(
        slot: &mut Option<Renderer>,
        holds: Holds,
        image: impl FnOnce() -> Result<Image, String>,
    ) -> Result<&mut StillRenderer, String> {
        let reuse = holds != Holds::Output && slot.as_ref().is_some_and(|r| r.holds == holds);
        if !reuse {
            let Image {
                width,
                height,
                rgba,
            } = image()?;
            match slot {
                Some(renderer) => {
                    renderer.still.set_image(width, height, rgba);
                    renderer.holds = holds;
                }
                None => {
                    *slot = Some(Renderer {
                        still: StillRenderer::new(width, height, rgba),
                        holds,
                    })
                }
            }
        }
        Ok(&mut slot.as_mut().expect("prepared").still)
    }
}

struct Session {
    capture: Arc<Capture>,
    renderer: Mutex<Option<Renderer>>,
    /// Edited images sent by the webview, by the token it sent them with.
    staged: Mutex<HashMap<u32, Image>>,
}

impl Session {
    /// The frame Beautify puts around a `width × height` image, rendered with a clear
    /// image so the webview can draw the markup into it.
    fn frame(
        &self,
        background: &BackgroundSettings,
        width: u32,
        height: u32,
        max_width: u32,
        max_height: u32,
    ) -> Result<(u32, u32, Vec<u8>), String> {
        Image::byte_len(width, height)?;
        let mut slot = self.renderer.lock().map_err(|e| e.to_string())?;
        let renderer = Renderer::prepare(&mut slot, Holds::Clear(width, height), || {
            Image::clear(width, height)
        })?;
        let (w, h) = recast_render::still::fit(
            renderer.native_size(background),
            max_width.clamp(16, MAX_SIDE),
            max_height.clamp(16, MAX_SIDE),
        );
        let pixels = renderer
            .render(background, w, h)
            .map_err(|e| e.to_string())?;
        Ok((w, h, pixels))
    }

    fn stage(&self, token: u32, image: Image) -> Result<(), String> {
        self.staged
            .lock()
            .map_err(|e| e.to_string())?
            .insert(token, image);
        Ok(())
    }

    /// The screenshot as PNG, on the Beautify background when one is given: the image
    /// staged with `token`, or the original when there is no token. A token whose image
    /// is missing is an error, so an edited screenshot never falls back to the original.
    fn output_png(
        &self,
        token: Option<u32>,
        background: Option<&BackgroundSettings>,
    ) -> Result<Vec<u8>, String> {
        let staged = match token {
            Some(token) => Some(
                self.staged
                    .lock()
                    .map_err(|e| e.to_string())?
                    .remove(&token)
                    .ok_or("the edited screenshot didn't arrive; try again")?,
            ),
            None => None,
        };
        let capture = &self.capture;
        match (staged, background) {
            (None, None) => Ok(capture.png.clone()),
            (Some(image), None) => bitmap::encode_png(image.width, image.height, &image.rgba)
                .map_err(|e| e.to_string()),
            (staged, Some(background)) => {
                let mut slot = self.renderer.lock().map_err(|e| e.to_string())?;
                let renderer = Renderer::prepare(&mut slot, Holds::Output, || match staged {
                    Some(image) => Ok(image),
                    None => Image::new(capture.width, capture.height, capture.rgba.clone()),
                })?;
                beautify(renderer, background)
            }
        }
    }
}

/// `renderer`'s image on `background` at its own pixel size, as PNG.
fn beautify(
    renderer: &mut StillRenderer,
    background: &BackgroundSettings,
) -> Result<Vec<u8>, String> {
    let (width, height) =
        recast_render::still::fit(renderer.native_size(background), MAX_SIDE, MAX_SIDE);
    let pixels = renderer
        .render(background, width, height)
        .map_err(|e| e.to_string())?;
    bitmap::encode_png(width, height, &pixels).map_err(|e| e.to_string())
}

#[derive(Default)]
pub struct Markups {
    windows: Mutex<HashMap<String, Arc<Session>>>,
    next_id: AtomicU64,
}

impl Markups {
    fn session(&self, label: &str) -> Result<Arc<Session>, String> {
        self.windows
            .lock()
            .map_err(|e| e.to_string())?
            .get(label)
            .cloned()
            .ok_or_else(|| "this window has no screenshot".to_string())
    }
}

pub fn open_window(app: &AppHandle, capture: Arc<Capture>) -> Result<(), String> {
    let markups = app.state::<Markups>();
    let label = format!(
        "{LABEL_PREFIX}{}",
        markups.next_id.fetch_add(1, Ordering::Relaxed)
    );
    let session = Arc::new(Session {
        capture: capture.clone(),
        renderer: Mutex::new(None),
        staged: Mutex::new(HashMap::new()),
    });
    markups
        .windows
        .lock()
        .map_err(|e| e.to_string())?
        .insert(label.clone(), session);
    // Also focuses the window, which the thumbnail can't do as it never activates the app.
    let window = crate::activation::build(app, &label, |builder| {
        builder
            .title(&capture.name)
            .inner_size(1280.0, 820.0)
            .min_inner_size(900.0, 560.0)
            .theme(Some(tauri::Theme::Dark))
    })?;
    let app = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            let removed = app
                .state::<Markups>()
                .windows
                .lock()
                .ok()
                .and_then(|mut windows| windows.remove(&label));
            // Releasing the GPU device can take a moment; keep it off the main thread.
            std::thread::spawn(move || drop(removed));
        }
    });
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MarkupInit {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    /// The Beautify background used last time.
    pub background: BackgroundSettings,
}

#[tauri::command]
#[specta::specta]
pub async fn markup_open(
    window: WebviewWindow,
    markups: State<'_, Markups>,
    settings: State<'_, SettingsStore>,
) -> Result<MarkupInit, String> {
    let capture = markups.session(window.label())?.capture.clone();
    Ok(MarkupInit {
        name: capture.name.clone(),
        width: capture.width,
        height: capture.height,
        scale_factor: capture.scale_factor,
        background: usable(settings.get().beautify),
    })
}

/// The screenshot as straight RGBA bytes, `width × height` from [`markup_open`].
#[tauri::command]
pub async fn markup_image(
    window: WebviewWindow,
    markups: State<'_, Markups>,
) -> Result<Response, String> {
    let capture = markups.session(window.label())?.capture.clone();
    Ok(Response::new(capture.rgba.clone()))
}

fn header(request: &Request<'_>, name: &str) -> Result<u32, String> {
    request
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| format!("missing {name}"))
}

/// Keeps the edited image, straight RGBA in the body with its size and a token in the
/// headers, for the [`markup_finish`] that names the token.
#[tauri::command]
pub async fn markup_stage(
    window: WebviewWindow,
    markups: State<'_, Markups>,
    request: Request<'_>,
) -> Result<(), String> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(
            "the edited screenshot arrived without its pixels (the IPC sent JSON, not raw bytes)"
                .into(),
        );
    };
    let image = Image::new(
        header(&request, WIDTH_HEADER)?,
        header(&request, HEIGHT_HEADER)?,
        bytes.clone(),
    )?;
    markups
        .session(window.label())?
        .stage(header(&request, TOKEN_HEADER)?, image)
}

/// Routes the commands that carry raw bytes, which the typed bindings can't describe,
/// and hands every other command to `typed`.
pub fn with_raw_commands(
    typed: impl Fn(Invoke<Wry>) -> bool + Send + Sync + 'static,
) -> impl Fn(Invoke<Wry>) -> bool + Send + Sync + 'static {
    let raw: fn(Invoke<Wry>) -> bool = tauri::generate_handler![markup_image, markup_stage];
    move |invoke| match invoke.message.command() {
        "markup_image" | "markup_stage" => raw(invoke),
        _ => typed(invoke),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MarkupFrame {
    /// PNG data URL.
    pub url: String,
    pub width: u32,
    pub height: u32,
}

/// The Beautify frame around a `width × height` image, without the image, scaled to fit
/// `max_width × max_height` pixels.
#[tauri::command]
#[specta::specta]
pub async fn markup_frame(
    window: WebviewWindow,
    markups: State<'_, Markups>,
    background: BackgroundSettings,
    width: u32,
    height: u32,
    max_width: u32,
    max_height: u32,
) -> Result<MarkupFrame, String> {
    let session = markups.session(window.label())?;
    tauri::async_runtime::spawn_blocking(move || {
        let (width, height, pixels) =
            session.frame(&background, width, height, max_width, max_height)?;
        let png = bitmap::encode_png(width, height, &pixels).map_err(|e| e.to_string())?;
        Ok(MarkupFrame {
            url: format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            ),
            width,
            height,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum MarkupAction {
    Copy,
    SaveAs {
        path: String,
    },
    /// Writes over the screenshot's file and closes the window.
    Done,
}

/// Carries out `action` with the finished `png`; returns the file written, if any.
fn deliver(
    action: &MarkupAction,
    png: &[u8],
    file: &Path,
    copy_on_done: bool,
    copy: impl Fn(&[u8]) -> Result<(), String>,
) -> Result<Option<PathBuf>, String> {
    match action {
        MarkupAction::Copy => copy(png).map(|_| None),
        MarkupAction::SaveAs { path } => {
            let path = PathBuf::from(path);
            std::fs::write(&path, png)
                .map_err(|e| format!("cannot save {}: {e}", path.display()))?;
            Ok(Some(path))
        }
        MarkupAction::Done => {
            replace(file, png).map_err(|e| format!("cannot save {}: {e}", file.display()))?;
            if copy_on_done {
                copy(png)?;
            }
            Ok(Some(file.to_path_buf()))
        }
    }
}

/// Writes `bytes` to a temporary file next to `path`, then renames it over `path`.
fn replace(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let temp = path.with_file_name(name);
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })
}

/// Finishes the image staged with `staged`, or the original screenshot when it is `None`,
/// with Beautify when `background` is given. Returns the file written, if any.
#[tauri::command]
#[specta::specta]
pub async fn markup_finish(
    window: WebviewWindow,
    markups: State<'_, Markups>,
    settings: State<'_, SettingsStore>,
    action: MarkupAction,
    staged: Option<u32>,
    background: Option<BackgroundSettings>,
) -> Result<Option<String>, String> {
    let session = markups.session(window.label())?;
    if let Some(background) = &background
        && let Err(e) = settings.update(|s| s.beautify = background.clone())
    {
        log::warn!("{e}");
    }
    let copy_on_done = settings.get().screenshots.copy_to_clipboard;
    let file = session.capture.current_file();
    let done = matches!(action, MarkupAction::Done);
    let written = tauri::async_runtime::spawn_blocking(move || {
        let png = session.output_png(staged, background.as_ref())?;
        deliver(&action, &png, &file, copy_on_done, copy_png)
    })
    .await
    .map_err(|e| e.to_string())??;
    if done && let Err(e) = window.close() {
        log::warn!("cannot close the markup window: {e}");
    }
    Ok(written.map(|p| p.display().to_string()))
}

/// `background`, with the default fill instead of an image that no longer exists.
fn usable(background: BackgroundSettings) -> BackgroundSettings {
    match &background.fill {
        BackgroundFill::Image { path } if !Path::new(path).is_file() => BackgroundSettings {
            fill: BackgroundSettings::default().fill,
            ..background
        },
        _ => background,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use recast_capture::Screenshot;
    use recast_project::{Color, Shadow};

    use super::*;

    fn checkerboard(width: u32, height: u32) -> Vec<u8> {
        (0..width * height)
            .flat_map(|i| {
                let (x, y) = (i % width, i / width);
                let v = if (x + y) % 2 == 0 { 0 } else { 255 };
                [v, v, v, 255]
            })
            .collect()
    }

    fn session(dir: &Path, width: u32, height: u32) -> Session {
        let shot = Screenshot {
            width,
            height,
            scale_factor: 2.0,
            rgba: checkerboard(width, height),
        };
        let capture = Capture::store(
            shot,
            "Recast t".into(),
            false,
            &dir.join("Pictures"),
            &dir.join("cache"),
        )
        .unwrap();
        Session {
            capture: Arc::new(capture),
            renderer: Mutex::new(None),
            staged: Mutex::new(HashMap::new()),
        }
    }

    fn decode(png: &[u8]) -> (u32, u32, Vec<u8>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.png");
        std::fs::write(&path, png).unwrap();
        bitmap::load_png(&path).unwrap()
    }

    fn plain_background(padding: f64) -> BackgroundSettings {
        BackgroundSettings {
            fill: BackgroundFill::Solid {
                color: Color::rgb(200, 30, 30),
            },
            padding,
            corner_radius: 0.0,
            shadow: Shadow {
                opacity: 0.0,
                ..Default::default()
            },
        }
    }

    #[test]
    fn without_edits_the_original_png_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let session = session(dir.path(), 40, 30);
        assert_eq!(session.output_png(None, None).unwrap(), session.capture.png);
    }

    #[test]
    fn each_finish_gets_the_image_staged_with_its_token() {
        let dir = tempfile::tempdir().unwrap();
        let session = session(dir.path(), 40, 30);
        let first = vec![7; 20 * 10 * 4];
        let second = vec![9; 8 * 4 * 4];
        session
            .stage(1, Image::new(20, 10, first.clone()).unwrap())
            .unwrap();
        session
            .stage(2, Image::new(8, 4, second.clone()).unwrap())
            .unwrap();
        assert_eq!(
            decode(&session.output_png(Some(1), None).unwrap()),
            (20, 10, first)
        );
        assert_eq!(
            decode(&session.output_png(Some(2), None).unwrap()),
            (8, 4, second)
        );
    }

    #[test]
    fn a_missing_staged_image_never_falls_back_to_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let session = session(dir.path(), 40, 30);
        session
            .stage(1, Image::new(2, 2, vec![7; 16]).unwrap())
            .unwrap();
        session.output_png(Some(1), None).unwrap();
        assert!(session.output_png(Some(1), None).is_err(), "used once");
        assert!(session.output_png(Some(5), None).is_err());
        assert!(
            session
                .output_png(Some(5), Some(&plain_background(0.1)))
                .is_err()
        );
    }

    #[test]
    fn image_bytes_must_match_the_size() {
        assert!(Image::new(2, 2, vec![0; 16]).is_ok());
        assert!(Image::new(2, 2, vec![0; 15]).is_err());
        assert!(Image::new(0, 2, vec![]).is_err());
    }

    #[test]
    fn sizes_past_the_texture_limit_are_refused_without_overflowing() {
        // 2^31 × 2^31 × 4 wraps to 0 in 64-bit arithmetic.
        assert!(Image::new(1 << 31, 1 << 31, vec![]).is_err());
        assert!(Image::new(u32::MAX, u32::MAX, vec![]).is_err());
        assert!(Image::byte_len(MAX_SIDE + 1, 1).is_err());
        assert_eq!(Image::byte_len(MAX_SIDE, 2).unwrap(), MAX_SIDE as usize * 8);

        let dir = tempfile::tempdir().unwrap();
        let session = session(dir.path(), 40, 30);
        let background = plain_background(0.0);
        assert!(
            session
                .frame(&background, 1 << 31, 1 << 31, 64, 64)
                .is_err()
        );
        assert!(session.frame(&background, MAX_SIDE + 1, 8, 64, 64).is_err());
        assert!(
            session.frame(&background, 40, 30, 64, 64).is_ok(),
            "still usable"
        );
    }

    #[test]
    fn a_cropped_image_keeps_every_pixel_inside_the_beautify_padding() {
        let dir = tempfile::tempdir().unwrap();
        let session = session(dir.path(), 400, 300);
        for (width, height, padding) in [(333, 211, 0.08), (157, 311, 0.05), (101, 57, 0.3)] {
            let cropped = checkerboard(width, height);
            session
                .stage(1, Image::new(width, height, cropped.clone()).unwrap())
                .unwrap();
            let background = plain_background(padding);
            let (out_w, out_h, pixels) =
                decode(&session.output_png(Some(1), Some(&background)).unwrap());
            let pad = (out_w - width) / 2;
            assert_eq!(out_h - height, 2 * pad);
            let off = (0..height)
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    let out = (((y + pad) * out_w + x + pad) * 4) as usize;
                    let src = ((y * width + x) * 4) as usize;
                    pixels[out].abs_diff(cropped[src]) > 1
                })
                .count();
            assert_eq!(off, 0, "{width}×{height} padding {padding}");
            assert_eq!(&pixels[..3], &[200, 30, 30], "the corner is background");
        }
    }

    #[test]
    fn frames_and_outputs_share_the_renderer() {
        let dir = tempfile::tempdir().unwrap();
        let session = session(dir.path(), 64, 48);
        let background = plain_background(0.25);
        let (w, h, frame) = session.frame(&background, 64, 48, 4096, 4096).unwrap();
        assert_eq!((w, h), (88, 72));
        let center = ((36 * w + 44) * 4) as usize;
        assert_eq!(&frame[center..center + 3], &[200, 30, 30], "a clear image");

        let (_, _, beautified) = decode(&session.output_png(None, Some(&background)).unwrap());
        let (x, y) = (12 + 33, 12 + 24);
        let at = ((y * w + x) * 4) as usize;
        let expected = if (33 + 24) % 2 == 0 { 0 } else { 255 };
        assert!(beautified[at].abs_diff(expected) <= 1, "the screenshot");

        let (_, _, again) = session.frame(&background, 64, 48, 4096, 4096).unwrap();
        assert_eq!(&again[center..center + 3], &[200, 30, 30]);
    }

    #[test]
    fn frames_fit_the_preview_size() {
        let dir = tempfile::tempdir().unwrap();
        let session = session(dir.path(), 64, 48);
        let (w, h, pixels) = session
            .frame(&plain_background(0.0), 400, 300, 200, 200)
            .unwrap();
        assert_eq!((w, h), (200, 150));
        assert_eq!(pixels.len(), 200 * 150 * 4);
    }

    #[test]
    fn done_writes_over_the_file_and_copies_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("shot.png");
        std::fs::write(&file, b"original").unwrap();
        let copied = RefCell::new(Vec::new());
        let copy = |png: &[u8]| {
            copied.borrow_mut().push(png.to_vec());
            Ok(())
        };

        let written = deliver(&MarkupAction::Done, b"edited", &file, true, copy).unwrap();
        assert_eq!(written, Some(file.clone()));
        assert_eq!(std::fs::read(&file).unwrap(), b"edited");
        assert_eq!(*copied.borrow(), [b"edited".to_vec()]);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "no temp");

        deliver(&MarkupAction::Done, b"again", &file, false, copy).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"again");
        assert_eq!(copied.borrow().len(), 1);
    }

    #[test]
    fn copy_and_save_as_leave_the_file_alone() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("shot.png");
        std::fs::write(&file, b"original").unwrap();
        let copied = RefCell::new(0);
        let copy = |_: &[u8]| {
            *copied.borrow_mut() += 1;
            Ok(())
        };

        assert_eq!(
            deliver(&MarkupAction::Copy, b"edited", &file, true, copy).unwrap(),
            None
        );
        let other = dir.path().join("other.png");
        let save_as = MarkupAction::SaveAs {
            path: other.display().to_string(),
        };
        assert_eq!(
            deliver(&save_as, b"edited", &file, true, copy).unwrap(),
            Some(other.clone())
        );
        assert_eq!(std::fs::read(&other).unwrap(), b"edited");
        assert_eq!(std::fs::read(&file).unwrap(), b"original");
        assert_eq!(*copied.borrow(), 1);
    }

    #[test]
    fn a_failed_copy_on_done_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("shot.png");
        let refuse = |_: &[u8]| Err("no clipboard".to_string());
        assert!(deliver(&MarkupAction::Done, b"edited", &file, true, refuse).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), b"edited");
    }

    #[test]
    fn a_missing_background_image_falls_back_to_the_default_fill() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("wallpaper.png");
        std::fs::write(&image, b"png").unwrap();
        let with_image = |path: &Path| BackgroundSettings {
            fill: BackgroundFill::Image {
                path: path.display().to_string(),
            },
            padding: 0.2,
            ..Default::default()
        };

        let kept = with_image(&image);
        assert_eq!(usable(kept.clone()), kept);

        let missing = usable(with_image(&dir.path().join("moved.png")));
        assert_eq!(missing.fill, BackgroundSettings::default().fill);
        assert_eq!(missing.padding, 0.2, "the rest is kept");
    }
}
