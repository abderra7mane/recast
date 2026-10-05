//! The compositor shared by preview and export: draws the background, the screen
//! recording with zoom, rounded corners and shadow, the cursor and click ripples.

pub mod bitmap;
mod compositor;
pub mod cursor;
pub mod layout;
pub mod nv12;
mod scene;

pub use compositor::Compositor;
pub use scene::{Scene, SceneParts, resolve};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("GPU: {0}")]
    Gpu(String),
    #[error("image: {0}")]
    Image(String),
    #[error("video frame: {0}")]
    Source(String),
    #[error(transparent)]
    Project(#[from] recast_project::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    Rgba8,
    Bgra8,
}

/// A video frame in CPU memory.
#[derive(Debug, Clone, Copy)]
pub struct CpuFrame<'a> {
    pub width: u32,
    pub height: u32,
    pub bytes_per_row: usize,
    pub format: PixelFormat,
    pub data: &'a [u8],
    /// Identifies the frame; a frame with the same id as the previous one is not uploaded again.
    pub id: Option<u64>,
}

/// Supplies the screen recording's frame for a time on the recording's timeline.
pub trait FrameSource {
    fn frame_at(&mut self, t_ms: f64) -> Result<CpuFrame<'_>>;
}
