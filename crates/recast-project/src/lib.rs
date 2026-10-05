mod bundle;
mod edits;
mod events;
pub mod mp4;
mod schema;
#[cfg(test)]
mod test_support;

pub use bundle::*;
pub use edits::*;
pub use events::*;
pub use schema::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid project.json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("cannot encode events: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("cannot decode events: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
    #[error("invalid media file: {0}")]
    InvalidMedia(String),
    #[error("unsupported edit settings version {0}")]
    UnsupportedEditsVersion(u32),
    #[error("unsupported project version {0}")]
    UnsupportedVersion(u32),
    #[error("{0} is not a Recast bundle")]
    NotABundle(String),
    #[error("the recording has no video frames")]
    NoVideo,
    #[error("the recording stopped before its metadata was written")]
    NoProject,
    #[error("the recording is still in progress (pid {0})")]
    StillRecording(u32),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
