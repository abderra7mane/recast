use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use recast_project::{BackgroundFill, Bundle, EditSettings, EventLog, Project, Rect, Resolution};
use recast_zoom::Timeline;

use crate::{
    Result,
    bitmap::Rgba,
    cursor::{CursorImage, default_arrow},
    layout::output_size,
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Everything needed to draw any frame of a recording, except the video itself.
#[derive(Debug, Clone)]
pub struct Scene {
    id: u64,
    pub settings: EditSettings,
    pub timeline: Timeline,
    /// Size of the recorded area in points.
    pub screen_width: f64,
    pub screen_height: f64,
    /// Size of the recording in pixels.
    pub video_width: u32,
    pub video_height: u32,
    bounds: Rect,
    cursors: HashMap<u32, CursorImage>,
    default_cursor: CursorImage,
    pub background: Option<Rgba>,
}

/// What a scene is built from.
pub struct SceneParts<'a> {
    pub bounds: Rect,
    pub video_size: (u32, u32),
    pub duration_ms: f64,
    pub events: &'a EventLog,
    pub settings: EditSettings,
    pub cursors: HashMap<u32, CursorImage>,
    pub background: Option<Rgba>,
}

impl Scene {
    pub fn new(parts: SceneParts<'_>) -> Self {
        let settings = parts.settings.sanitized(parts.duration_ms);
        let timeline = Timeline::new(parts.events, &parts.bounds, &settings, parts.duration_ms);
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            settings,
            timeline,
            screen_width: parts.bounds.width,
            screen_height: parts.bounds.height,
            video_width: parts.video_size.0,
            video_height: parts.video_size.1,
            bounds: parts.bounds,
            cursors: parts.cursors,
            default_cursor: default_arrow(4.0),
            background: parts.background,
        }
    }

    /// Loads events, cursor images and the background image of a bundle.
    /// `settings` replaces the bundle's saved edits when given.
    pub fn load(bundle: &Bundle, settings: Option<EditSettings>) -> Result<Self> {
        let project = bundle.load_project()?;
        let events = bundle.load_events()?;
        let settings = settings.unwrap_or_else(|| project.edits.clone());
        Self::from_project(bundle.path(), &project, &events, settings)
    }

    pub fn from_project(
        bundle_dir: &Path,
        project: &Project,
        events: &EventLog,
        settings: EditSettings,
    ) -> Result<Self> {
        let mut cursors = HashMap::new();
        for shape in &events.cursor_shapes {
            match Rgba::load(&bundle_dir.join(&shape.file)) {
                Ok(image) => {
                    cursors.insert(
                        shape.id,
                        CursorImage {
                            image,
                            width_pts: shape.width,
                            height_pts: shape.height,
                            hotspot_x: shape.hotspot_x,
                            hotspot_y: shape.hotspot_y,
                        },
                    );
                }
                Err(e) => log::warn!("cursor {} is unusable: {e}", shape.id),
            }
        }
        let background = match &settings.background.fill {
            BackgroundFill::Image { path } => Some(Rgba::load(&resolve(bundle_dir, path))?),
            _ => None,
        };
        let recording = &project.recording;
        Ok(Self::new(SceneParts {
            bounds: recording.bounds,
            video_size: (recording.width, recording.height),
            duration_ms: recording.duration_ms,
            events,
            settings,
            cursors,
            background,
        }))
    }

    /// Changes whenever the scene's images change, so renderers know to reload them.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Applies new settings; the timeline is rebuilt only when its settings changed.
    /// A new background image path needs [`Scene::set_background`] too.
    pub fn set_settings(&mut self, events: &EventLog, settings: EditSettings) {
        let duration_ms = self.timeline.duration_ms();
        let settings = settings.sanitized(duration_ms);
        if settings.zoom != self.settings.zoom || settings.cursor != self.settings.cursor {
            self.timeline = Timeline::new(events, &self.bounds, &settings, duration_ms);
        }
        let image = |s: &EditSettings| match &s.background.fill {
            BackgroundFill::Image { path } => Some(path.clone()),
            _ => None,
        };
        if image(&settings) != image(&self.settings) {
            self.id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        }
        self.settings = settings;
    }

    pub fn set_background(&mut self, background: Option<Rgba>) {
        self.background = background;
        self.id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    }

    /// The cursor image for a recorded shape, or the default arrow.
    pub fn cursor(&self, shape: Option<u32>) -> (&CursorImage, Option<u32>) {
        match shape.and_then(|id| self.cursors.get(&id).map(|c| (c, id))) {
            Some((image, id)) => (image, Some(id)),
            None => (&self.default_cursor, None),
        }
    }

    pub fn cursor_images(&self) -> impl Iterator<Item = (Option<u32>, &CursorImage)> {
        self.cursors
            .iter()
            .map(|(id, c)| (Some(*id), c))
            .chain(std::iter::once((None, &self.default_cursor)))
    }

    /// Output size for the export settings.
    pub fn output_size(&self) -> (u32, u32) {
        self.output_size_for(self.settings.export.resolution)
    }

    pub fn output_size_for(&self, resolution: Resolution) -> (u32, u32) {
        output_size(
            self.video_width,
            self.video_height,
            self.settings.background.padding,
            resolution,
        )
    }
}

/// Resolves a background image path, which may be relative to the bundle.
pub fn resolve(bundle_dir: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        bundle_dir.join(path)
    }
}
