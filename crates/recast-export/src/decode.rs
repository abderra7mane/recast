use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use cidre::{arc, av, cm, cv, ns};
use recast_render::{CpuFrame, FrameSource, PixelFormat};

use crate::{Error, Result, SAMPLE_RATE};

fn load_track(path: &Path, media_type: &av::MediaType) -> Result<Option<arc::R<av::AssetTrack>>> {
    let url = ns::Url::with_fs_path_str(&path.to_string_lossy(), false);
    let asset = av::UrlAsset::with_url(&url, None)
        .ok_or_else(|| Error::Media(format!("cannot open {}", path.display())))?;
    let tracks = pollster::block_on(asset.load_tracks_with_media_type(media_type))
        .map_err(|e| Error::Media(format!("{}: {}", path.display(), e.localized_desc())))?;
    Ok(tracks.get(0).ok())
}

fn reader(
    track: &av::AssetTrack,
    settings: &ns::Dictionary<ns::String, ns::Id>,
    start: Option<f64>,
) -> Result<(arc::R<av::AssetReader>, arc::R<av::AssetReaderTrackOutput>)> {
    let mut reader = av::AssetReader::with_asset(track.asset())
        .map_err(|e| Error::Media(format!("cannot read media: {}", e.localized_desc())))?;
    let mut output = av::AssetReaderTrackOutput::with_track(track, Some(settings))
        .map_err(|e| Error::Media(format!("unsupported media: {e:?}")))?;
    output.set_always_copies_sample_data(false);
    reader
        .add_output(&output)
        .map_err(|e| Error::Media(format!("cannot read track: {e:?}")))?;
    if let Some(start) = start.filter(|s| *s > 0.0) {
        reader
            .set_time_range(cm::TimeRange {
                start: cm::Time::with_secs(start, 600),
                duration: cm::Time::infinity(),
            })
            .map_err(|e| Error::Media(format!("cannot seek: {e:?}")))?;
    }
    match reader.start_reading() {
        Ok(true) => Ok((reader, output)),
        _ => Err(Error::Media(format!(
            "cannot start reading: {}",
            reader
                .error()
                .map(|e| e.localized_desc().to_string())
                .unwrap_or_default()
        ))),
    }
}

fn next_sample(
    reader: &av::AssetReader,
    output: &mut av::AssetReaderTrackOutput,
) -> Result<Option<arc::R<cm::SampleBuf>>> {
    let next = output
        .next_sample_buf()
        .map_err(|e| Error::Media(format!("cannot read sample: {e:?}")))?;
    if next.is_none() && reader.status() == av::AssetReaderStatus::Failed {
        return Err(Error::Media(format!(
            "decoding failed: {}",
            reader
                .error()
                .map(|e| e.localized_desc().to_string())
                .unwrap_or_default()
        )));
    }
    Ok(next)
}

/// Frame ids are unique in the process, so a renderer never mistakes a frame from
/// one decoder for a frame from another.
static NEXT_FRAME_ID: AtomicU64 = AtomicU64::new(1);

/// A decoded BGRA frame whose pixels stay locked while it is held.
struct LockedFrame {
    pixels: arc::R<cv::PixelBuf>,
    pts_ms: f64,
    id: u64,
}

impl LockedFrame {
    fn new(sample: &cm::SampleBuf) -> Result<Self> {
        let mut pixels = sample
            .image_buf()
            .ok_or_else(|| Error::Media("video sample without an image".into()))?
            .retained();
        // SAFETY: unlocked in `drop`; the buffer is only read while locked.
        unsafe { pixels.lock_base_addr(cv::pixel_buffer::LockFlags::READ_ONLY) }
            .result()
            .map_err(|e| Error::Media(format!("cannot lock frame: {e:?}")))?;
        Ok(Self {
            pixels,
            pts_ms: sample.pts().as_secs() * 1000.0,
            id: NEXT_FRAME_ID.fetch_add(1, Ordering::Relaxed),
        })
    }

    fn frame(&self) -> CpuFrame<'_> {
        let height = self.pixels.height();
        let stride = self.pixels.bytes_per_row();
        // SAFETY: the buffer is locked for reading and holds `stride * height` bytes.
        let data = unsafe {
            std::slice::from_raw_parts(self.pixels.base_address() as *const u8, stride * height)
        };
        CpuFrame {
            width: self.pixels.width() as u32,
            height: height as u32,
            bytes_per_row: stride,
            format: PixelFormat::Bgra8,
            data,
            id: Some(self.id),
            has_alpha: false,
        }
    }
}

impl Drop for LockedFrame {
    fn drop(&mut self) {
        // SAFETY: matches the lock in `new`.
        let _ = unsafe {
            self.pixels
                .unlock_lock_base_addr(cv::pixel_buffer::LockFlags::READ_ONLY)
        };
    }
}

/// Reads the screen recording in order. The recording has a variable frame rate,
/// so each requested time shows the last frame that started at or before it.
pub struct VideoDecoder {
    reader: arc::R<av::AssetReader>,
    output: arc::R<av::AssetReaderTrackOutput>,
    current: Option<LockedFrame>,
    next: Option<LockedFrame>,
    ended: bool,
}

/// Frames up to this much later than the requested time still count as on time, so
/// rounding in timestamps does not show the previous frame.
const TOLERANCE_MS: f64 = 0.5;

impl VideoDecoder {
    pub fn open(path: &Path, start_ms: f64) -> Result<Self> {
        let track = load_track(path, av::MediaType::video())?
            .ok_or_else(|| Error::Media(format!("{} has no video", path.display())))?;
        let mut settings = ns::DictionaryMut::<ns::String, ns::Id>::with_capacity(1);
        settings.insert(
            cv::pixel_buffer_keys::pixel_format().as_ns(),
            ns::Number::with_u32(cv::PixelFormat::_32_BGRA.0).as_id_ref(),
        );
        let (reader, output) = reader(&track, &settings, Some(start_ms / 1000.0))?;
        Ok(Self {
            reader,
            output,
            current: None,
            next: None,
            ended: false,
        })
    }

    fn read(&mut self) -> Result<Option<LockedFrame>> {
        if self.ended {
            return Ok(None);
        }
        match next_sample(&self.reader, &mut self.output)? {
            Some(sample) => LockedFrame::new(&sample).map(Some),
            None => {
                self.ended = true;
                Ok(None)
            }
        }
    }

    /// Presentation time of the frame shown last.
    pub fn current_ms(&self) -> Option<f64> {
        self.current.as_ref().map(|f| f.pts_ms)
    }

    fn advance_to(&mut self, t_ms: f64) -> Result<()> {
        if self.next.is_none() {
            self.next = self.read()?;
        }
        while let Some(next) = &self.next {
            if next.pts_ms > t_ms + TOLERANCE_MS && self.current.is_some() {
                break;
            }
            self.current = self.next.take();
            self.next = self.read()?;
        }
        Ok(())
    }
}

impl FrameSource for VideoDecoder {
    fn frame_at(&mut self, t_ms: f64) -> recast_render::Result<CpuFrame<'_>> {
        self.advance_to(t_ms)
            .map_err(|e| recast_render::Error::Source(e.to_string()))?;
        self.current
            .as_ref()
            .map(LockedFrame::frame)
            .ok_or_else(|| recast_render::Error::Source("the recording has no frames".into()))
    }
}

/// Reads an audio file as interleaved stereo float samples at 48 kHz.
pub struct AudioDecoder {
    reader: arc::R<av::AssetReader>,
    output: arc::R<av::AssetReaderTrackOutput>,
    pending: Vec<f32>,
    pos: usize,
    ended: bool,
    /// Where reading starts, until the first buffer lines up with it.
    start_ms: Option<f64>,
}

impl AudioDecoder {
    pub fn open(path: &Path) -> Result<Option<Self>> {
        Self::open_at(path, 0.0)
    }

    /// Opens `path` so that the first sample read is the one at `start_ms`.
    pub fn open_at(path: &Path, start_ms: f64) -> Result<Option<Self>> {
        let Some(track) = load_track(path, av::MediaType::audio())? else {
            return Ok(None);
        };
        use av::audio::{all_formats_keys as all, linear_pcm_keys as pcm};
        let mut settings = ns::DictionaryMut::<ns::String, ns::Id>::with_capacity(7);
        settings.insert(
            all::id(),
            ns::Number::with_u32(u32::from_be_bytes(*b"lpcm")).as_id_ref(),
        );
        settings.insert(
            all::sample_rate(),
            ns::Number::with_f64(SAMPLE_RATE as f64).as_id_ref(),
        );
        settings.insert(
            all::number_of_channels(),
            ns::Number::with_u32(2).as_id_ref(),
        );
        settings.insert(pcm::bit_depth(), ns::Number::with_u32(32).as_id_ref());
        settings.insert(pcm::is_float(), ns::Number::with_bool(true).as_id_ref());
        settings.insert(
            pcm::is_non_interleaved(),
            ns::Number::with_bool(false).as_id_ref(),
        );
        settings.insert(
            pcm::is_big_endian(),
            ns::Number::with_bool(false).as_id_ref(),
        );
        let start_ms = start_ms.max(0.0);
        let (reader, output) = reader(&track, &settings, Some(start_ms / 1000.0))?;
        Ok(Some(Self {
            reader,
            output,
            pending: Vec::new(),
            pos: 0,
            ended: false,
            start_ms: Some(start_ms),
        }))
    }

    fn refill(&mut self) -> Result<bool> {
        if self.ended {
            return Ok(false);
        }
        let Some(sample) = next_sample(&self.reader, &mut self.output)? else {
            self.ended = true;
            return Ok(false);
        };
        let Some(block) = sample.data_buf() else {
            return Ok(true);
        };
        let mut bytes = vec![0u8; block.data_len()];
        block
            .copy_to(0, &mut bytes)
            .map_err(|e| Error::Media(format!("cannot read audio: {e:?}")))?;
        self.pending.clear();
        self.pos = 0;
        if let Some(start_ms) = self.start_ms.take() {
            let pts_ms = sample.pts().as_secs() * 1000.0;
            let offset = ((start_ms - pts_ms) * SAMPLE_RATE as f64 / 1000.0).round() as i64 * 2;
            if offset < 0 {
                self.pending.resize((-offset) as usize, 0.0);
            } else {
                self.pos = offset as usize;
            }
        }
        self.pending.extend(
            bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        );
        if self.pos > self.pending.len() {
            let rest = self.pos - self.pending.len();
            self.pending.clear();
            self.pos = 0;
            self.skip(rest)?;
        }
        Ok(true)
    }

    /// Fills `out` with the next samples, padding with silence after the end.
    pub fn read(&mut self, out: &mut [f32]) -> Result<()> {
        let mut filled = 0;
        while filled < out.len() {
            if self.pos >= self.pending.len() && !self.refill()? {
                out[filled..].fill(0.0);
                return Ok(());
            }
            let n = (self.pending.len() - self.pos).min(out.len() - filled);
            out[filled..filled + n].copy_from_slice(&self.pending[self.pos..self.pos + n]);
            self.pos += n;
            filled += n;
        }
        Ok(())
    }

    /// Discards the next `samples` samples.
    pub fn skip(&mut self, mut samples: usize) -> Result<()> {
        let mut scratch = vec![0.0; 4096];
        while samples > 0 {
            let n = samples.min(scratch.len());
            self.read(&mut scratch[..n])?;
            samples -= n;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{thread::sleep, time::Duration};

    use recast_capture::macos::{synthetic, writer::MediaWriter};

    use super::*;

    /// 60 fps for 3 s from host time 1000 s, with no frames between 1 s and 2 s.
    fn variable_rate_video(path: &Path) {
        let base = cm::Time::new(1_000, 1);
        let mut writer = MediaWriter::hevc(path, 64, 48, 60).unwrap();
        for i in (0..180).filter(|i| !(60..120).contains(i)) {
            let pts = base.add(cm::Time::new(i, 60));
            let buf = synthetic::video_frame(64, 48, i as u64, pts, cm::Time::new(1, 60)).unwrap();
            while !writer.is_ready() {
                sleep(Duration::from_millis(1));
            }
            writer.append(&buf).unwrap();
        }
        assert!(
            writer
                .finish_at(Some(base.add(cm::Time::new(3, 1))))
                .unwrap()
        );
    }

    /// The 60 fps frame number shown at `t_ms`.
    fn shown(decoder: &mut VideoDecoder, t_ms: f64) -> i64 {
        decoder.frame_at(t_ms).unwrap();
        (decoder.current_ms().unwrap() * 60.0 / 1000.0).round() as i64
    }

    #[test]
    fn holds_the_last_frame_through_gaps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("screen.mp4");
        variable_rate_video(&path);

        let mut decoder = VideoDecoder::open(&path, 0.0).unwrap();
        assert!(decoder.frame_at(0.0).unwrap().id.is_some());
        assert!(decoder.current_ms().unwrap().abs() < 1e-6);
        assert_eq!(shown(&mut decoder, 1000.0 / 60.0), 1);
        assert_eq!(shown(&mut decoder, 1000.0 / 60.0 * 1.5), 1);
        assert_eq!(shown(&mut decoder, 990.0), 59);
        assert_eq!(shown(&mut decoder, 1_500.0), 59);
        assert_eq!(shown(&mut decoder, 1_999.0), 59);
        assert_eq!(shown(&mut decoder, 2_000.0), 120);
        let frame = decoder.frame_at(2_500.0).unwrap();
        assert_eq!((frame.width, frame.height), (64, 48));
        assert!(frame.bytes_per_row >= 64 * 4);
        assert_eq!(shown(&mut decoder, 10_000.0), 179);
    }

    #[test]
    fn frame_ids_differ_between_decoders() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("screen.mp4");
        variable_rate_video(&path);
        let mut a = VideoDecoder::open(&path, 0.0).unwrap();
        let mut b = VideoDecoder::open(&path, 0.0).unwrap();
        assert_ne!(a.frame_at(0.0).unwrap().id, b.frame_at(0.0).unwrap().id);
    }

    #[test]
    fn starts_near_the_trim_point() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("screen.mp4");
        variable_rate_video(&path);
        let mut decoder = VideoDecoder::open(&path, 2_500.0).unwrap();
        decoder.frame_at(2_500.0).unwrap();
        let pts = decoder.current_ms().unwrap();
        assert!((pts - 2_500.0).abs() <= 1000.0 / 60.0, "{pts}");
    }

    #[test]
    fn audio_opened_at_a_time_matches_skipping() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audio.m4a");
        let base = cm::Time::new(1_000, 1);
        let mut writer = MediaWriter::aac(&path, 2).unwrap();
        for i in 0..(2 * 48_000 / 1024) {
            let first = i * 1024;
            let pts = base.add(cm::Time::new(first as i64, 48_000));
            let buf = synthetic::audio_chunk(48_000.0, first, 1024, pts).unwrap();
            while !writer.is_ready() {
                sleep(Duration::from_millis(1));
            }
            writer.append(&buf).unwrap();
        }
        assert!(writer.finish().unwrap());

        let mut skipped = AudioDecoder::open(&path).unwrap().unwrap();
        skipped.skip(1_234 * 96).unwrap();
        let mut expected = vec![0.0; 9_600];
        skipped.read(&mut expected).unwrap();
        let mut seeked = AudioDecoder::open_at(&path, 1_234.0).unwrap().unwrap();
        let mut actual = vec![0.0; 9_600];
        seeked.read(&mut actual).unwrap();

        assert!(expected.iter().any(|s| s.abs() > 0.05));
        let worst = expected
            .iter()
            .zip(&actual)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(worst < 0.02, "largest difference {worst}");
    }
}
