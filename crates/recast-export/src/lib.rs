//! Exports a recording to MP4: decodes the screen video, renders every frame with
//! the compositor, mixes audio and click sounds, and encodes with AVAssetWriter.

#[cfg(target_os = "macos")]
pub mod decode;
#[cfg(target_os = "macos")]
mod encode;
#[cfg(target_os = "macos")]
pub mod mix;
pub mod sounds;

use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    thread::sleep,
    time::{Duration, Instant},
};

use recast_project::{Bundle, EditSettings};
use recast_render::{Compositor, PixelFormat, Scene};

pub const SAMPLE_RATE: u32 = 48_000;
/// Audio is written up to this far ahead of the video, so the writer can interleave.
const AUDIO_LEAD_MS: f64 = 500.0;
const AUDIO_CHUNK_FRAMES: usize = 4_800;
/// Longest wait for the encoder to accept data before giving up.
const STALL_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Project(#[from] recast_project::Error),
    #[error(transparent)]
    Render(#[from] recast_render::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Media(String),
    #[error("{0}")]
    Audio(String),
    #[error("the trimmed recording is empty")]
    Empty,
    #[error("export cancelled")]
    Cancelled,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub frame: u64,
    pub total_frames: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportSummary {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub frames: u64,
    pub duration_ms: f64,
    pub audio: bool,
    pub elapsed: Duration,
}

pub struct ExportRequest<'a> {
    pub bundle: &'a Path,
    pub output: &'a Path,
    /// Replaces the edits saved in the bundle when given.
    pub settings: Option<EditSettings>,
}

/// Exports `request.bundle` to `request.output`, calling `on_progress` after each
/// frame. Setting `cancel` stops the export and removes the partial file.
#[cfg(target_os = "macos")]
pub fn export(
    request: &ExportRequest<'_>,
    on_progress: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<ExportSummary> {
    use decode::VideoDecoder;
    use encode::{Encoder, EncoderOptions, bitrate};
    use mix::{Mixer, TrackInput};

    let started = Instant::now();
    let bundle = Bundle::open(request.bundle)?;
    let project = bundle.load_project()?;
    let scene = Scene::load(&bundle, request.settings.clone())?;
    let settings = &scene.settings;
    let recording = &project.recording;
    let (start_ms, end_ms) = settings.trim_range(recording.duration_ms);
    let duration_ms = end_ms - start_ms;
    let fps = settings.export.fps.fps();
    let total_frames = (duration_ms * fps as f64 / 1000.0).round() as u64;
    if total_frames == 0 {
        return Err(Error::Empty);
    }
    let (width, height) = scene.output_size();
    let mut compositor = Compositor::new(width, height, PixelFormat::Bgra8)?;
    let mut video = VideoDecoder::open(&bundle.file(&recording.video.file), start_ms)?;

    let mut tracks = Vec::new();
    for (track, gain) in [
        (&recording.system_audio, settings.audio.system_volume),
        (&recording.mic, settings.audio.mic_volume),
    ] {
        let Some(track) = track else { continue };
        tracks.push(TrackInput {
            path: bundle.file(&track.file),
            offset_ms: track.offset_ms,
            gain: gain as f32,
        });
    }
    let audio_frames = (total_frames as f64 * SAMPLE_RATE as f64 / fps as f64).round() as u64;
    let mut mixer = Mixer::new(
        tracks,
        scene.timeline.clicks(),
        &settings.sounds,
        start_ms,
        audio_frames,
    )?;
    let has_audio = mixer.has_audio();

    let mut encoder = Encoder::create(
        request.output,
        &EncoderOptions {
            width,
            height,
            fps,
            codec: settings.export.codec,
            bitrate: bitrate(
                width,
                height,
                fps,
                settings.export.codec,
                settings.export.quality,
            ),
            audio: has_audio,
        },
    )?;

    let mut audio = AudioFeed {
        mixer: &mut mixer,
        pending: None,
        done: !has_audio,
    };
    let result = (|| -> Result<()> {
        let row = width as usize * 4;
        for frame in 0..total_frames {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let t_ms = start_ms + frame as f64 * 1000.0 / fps as f64;
            compositor.render_from(&scene, &mut video, t_ms)?;

            let audio_until = ((frame + 1) as f64 * 1000.0 / fps as f64 + AUDIO_LEAD_MS)
                * SAMPLE_RATE as f64
                / 1000.0;
            audio.feed(&mut encoder, audio_until as u64)?;
            let waited = Instant::now();
            while !encoder.video_ready() {
                if !audio.feed_one(&mut encoder)? {
                    sleep(Duration::from_millis(1));
                }
                if cancel.load(Ordering::Relaxed) {
                    return Err(Error::Cancelled);
                }
                if waited.elapsed() > STALL_TIMEOUT {
                    return Err(Error::Media("the encoder stopped accepting frames".into()));
                }
            }
            compositor.read(|data, stride| {
                encoder.append_frame(frame, |dst, dst_stride| {
                    for y in 0..height as usize {
                        dst[y * dst_stride..y * dst_stride + row]
                            .copy_from_slice(&data[y * stride..y * stride + row]);
                    }
                })
            })??;
            on_progress(Progress {
                frame: frame + 1,
                total_frames,
            });
        }
        let waited = Instant::now();
        while !audio.done {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            if !audio.feed_one(&mut encoder)? {
                sleep(Duration::from_millis(1));
            }
            if waited.elapsed() > STALL_TIMEOUT {
                return Err(Error::Media("the encoder stopped accepting audio".into()));
            }
        }
        Ok(())
    })();

    if let Err(e) = result {
        encoder.cancel();
        let _ = std::fs::remove_file(request.output);
        return Err(e);
    }
    if let Err(e) = encoder.finish(total_frames) {
        let _ = std::fs::remove_file(request.output);
        return Err(e);
    }
    Ok(ExportSummary {
        width,
        height,
        fps,
        frames: total_frames,
        duration_ms: total_frames as f64 * 1000.0 / fps as f64,
        audio: has_audio,
        elapsed: started.elapsed(),
    })
}

#[cfg(target_os = "macos")]
struct AudioFeed<'a> {
    mixer: &'a mut mix::Mixer,
    pending: Option<(u64, Vec<f32>)>,
    done: bool,
}

#[cfg(target_os = "macos")]
impl AudioFeed<'_> {
    /// Appends one chunk if the encoder takes it; returns whether it did.
    fn feed_one(&mut self, encoder: &mut encode::Encoder) -> Result<bool> {
        if self.done || !encoder.audio_ready() {
            return Ok(false);
        }
        if self.pending.is_none() {
            self.pending = self.mixer.next_chunk(AUDIO_CHUNK_FRAMES)?;
        }
        match self.pending.take() {
            Some((first, samples)) => {
                encoder.append_audio(first, &samples)?;
                Ok(true)
            }
            None => {
                encoder.finish_audio();
                self.done = true;
                Ok(false)
            }
        }
    }

    /// Appends chunks while the encoder takes them, up to audio frame `until`.
    fn feed(&mut self, encoder: &mut encode::Encoder, until: u64) -> Result<()> {
        while !self.done && self.next_first() < until && self.feed_one(encoder)? {}
        Ok(())
    }

    fn next_first(&mut self) -> u64 {
        match &self.pending {
            Some((first, _)) => *first,
            None => self.mixer.position(),
        }
    }
}
