//! Still images (screenshots) drawn on a background with padding, rounded corners and a
//! shadow, by the same compositor as recordings.

use std::path::Path;

use recast_project::{BackgroundFill, BackgroundSettings, EditSettings, EventLog, Rect};

use crate::{Compositor, CpuFrame, PixelFormat, Result, Scene, SceneParts, bitmap::Rgba};

/// `padding` (a fraction of the shorter side) in whole pixels for a `width × height` image.
pub fn padding_pixels(width: u32, height: u32, padding: f64) -> u32 {
    (padding.clamp(0.0, 0.5) * width.min(height) as f64).round() as u32
}

/// Frame size that shows a `width × height` pixel image at its own size, with `padding`
/// (a fraction of its shorter side, rounded to whole pixels) around it.
pub fn native_size(width: u32, height: u32, padding: f64) -> (u32, u32) {
    let pad = padding_pixels(width, height, padding);
    (width + 2 * pad, height + 2 * pad)
}

/// `size` scaled down to fit `max_width × max_height`, keeping its aspect ratio.
pub fn fit(size: (u32, u32), max_width: u32, max_height: u32) -> (u32, u32) {
    let scale = (max_width as f64 / size.0 as f64)
        .min(max_height as f64 / size.1 as f64)
        .min(1.0);
    (
        ((size.0 as f64 * scale).round() as u32).max(1),
        ((size.1 as f64 * scale).round() as u32).max(1),
    )
}

fn background_image(background: &BackgroundSettings) -> Option<&str> {
    match &background.fill {
        BackgroundFill::Image { path } => Some(path),
        _ => None,
    }
}

/// Renders one image again and again with changing background settings, keeping the GPU
/// device and textures between renders.
pub struct StillRenderer {
    image: Rgba,
    compositor: Option<Compositor>,
    scene: Option<Scene>,
    background_path: Option<String>,
}

impl StillRenderer {
    /// `rgba` is straight RGBA, tightly packed.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            image: Rgba::from_straight(width, height, rgba),
            compositor: None,
            scene: None,
            background_path: None,
        }
    }

    pub fn image_size(&self) -> (u32, u32) {
        (self.image.width, self.image.height)
    }

    /// Size of the frame showing the image at its own pixel size.
    pub fn native_size(&self, background: &BackgroundSettings) -> (u32, u32) {
        native_size(self.image.width, self.image.height, background.padding)
    }

    /// Renders the image on `background` into a `width × height` frame and returns its
    /// straight RGBA pixels. A background image path must be absolute.
    pub fn render(
        &mut self,
        background: &BackgroundSettings,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>> {
        let (image_w, image_h) = (self.image.width, self.image.height);
        // The padding the frame size was made with, so the image lands on whole pixels
        // at its own size instead of being resampled.
        let mut settings = EditSettings {
            background: BackgroundSettings {
                padding: padding_pixels(image_w, image_h, background.padding) as f64
                    / image_w.min(image_h) as f64,
                ..background.clone()
            },
            ..Default::default()
        };
        settings.zoom.auto = false;
        let path = background_image(background).map(str::to_string);
        let image = match (&path, path == self.background_path) {
            (Some(path), false) => Some(Some(Rgba::load(Path::new(path))?)),
            (None, false) => Some(None),
            (_, true) => None,
        };

        let scene = match &mut self.scene {
            Some(scene) => {
                scene.set_settings(&EventLog::default(), settings);
                if let Some(image) = image {
                    scene.set_background(image);
                }
                scene
            }
            None => self.scene.insert(Scene::new(SceneParts {
                bounds: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: self.image.width as f64,
                    height: self.image.height as f64,
                },
                duration_ms: 0.0,
                events: &EventLog::default(),
                settings,
                cursors: Default::default(),
                background: image.flatten(),
            })),
        };
        self.background_path = path;

        let compositor = match &mut self.compositor {
            Some(compositor) => {
                compositor.resize(width, height)?;
                compositor
            }
            None => self
                .compositor
                .insert(Compositor::new(width, height, PixelFormat::Rgba8)?),
        };
        let frame = CpuFrame {
            width: self.image.width,
            height: self.image.height,
            bytes_per_row: self.image.width as usize * 4,
            format: PixelFormat::Rgba8,
            data: &self.image.pixels,
            id: Some(0),
            has_alpha: true,
        };
        compositor.render(scene, &frame, 0.0)?;
        compositor.read_packed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_size_adds_padding_of_the_shorter_side() {
        assert_eq!(native_size(400, 300, 0.0), (400, 300));
        assert_eq!(native_size(400, 300, 0.1), (460, 360));
        assert_eq!(native_size(300, 400, 0.1), (360, 460));
        assert_eq!(native_size(400, 300, 9.0), (700, 600));
    }

    #[test]
    fn fit_only_scales_down() {
        assert_eq!(fit((2000, 1000), 1000, 1000), (1000, 500));
        assert_eq!(fit((1000, 2000), 1000, 1000), (500, 1000));
        assert_eq!(fit((300, 200), 1000, 1000), (300, 200));
        assert_eq!(fit((5000, 3), 100, 100), (100, 1));
    }
}
