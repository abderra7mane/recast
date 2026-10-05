use std::{path::Path, sync::mpsc, time::Duration};

use cidre::{arc, av, cm, ns, objc::Obj};

use crate::{Error, Result, video_bitrate};

const FRAGMENT_SECONDS: f64 = 2.0;

/// Wraps one AVAssetWriter with a single input, writing a fragmented MP4 so a crash
/// leaves a readable file.
pub struct MediaWriter {
    writer: arc::R<av::AssetWriter>,
    input: arc::R<av::AssetWriterInput>,
    started: bool,
    end: Option<cm::Time>,
    last_frame: Option<arc::R<cm::SampleBuf>>,
    /// Used for samples that carry no duration (screen frames may not).
    nominal_duration: Option<cm::Time>,
    dropped: u64,
}

// AVAssetWriter is safe to use from one thread at a time; callers keep it behind a lock.
unsafe impl Send for MediaWriter {}

impl MediaWriter {
    pub fn hevc(path: &Path, width: u32, height: u32, fps: u32) -> Result<Self> {
        let mut compression = ns::DictionaryMut::<ns::String, ns::Id>::with_capacity(5);
        compression.insert(
            ns::str!(c"AverageBitRate"),
            ns::Number::with_u32(video_bitrate(width, height, fps)).as_id_ref(),
        );
        compression.insert(
            ns::str!(c"ExpectedFrameRate"),
            ns::Number::with_u32(fps).as_id_ref(),
        );
        compression.insert(
            ns::str!(c"MaxKeyFrameIntervalDuration"),
            ns::Number::with_f64(FRAGMENT_SECONDS).as_id_ref(),
        );
        compression.insert(
            ns::str!(c"AllowFrameReordering"),
            ns::Number::with_bool(false).as_id_ref(),
        );
        compression.insert(
            ns::str!(c"ProfileLevel"),
            ns::str!(c"HEVC_Main_AutoLevel").as_id_ref(),
        );

        use av::video_settings_keys as keys;
        let mut settings = ns::DictionaryMut::<ns::String, ns::Id>::with_capacity(4);
        settings.insert(keys::codec(), av::VideoCodec::hevc().as_id_ref());
        settings.insert(keys::width(), ns::Number::with_u32(width).as_id_ref());
        settings.insert(keys::height(), ns::Number::with_u32(height).as_id_ref());
        settings.insert(keys::compression_props(), compression.as_id_ref());

        let mut writer = Self::new(path, av::FileType::mp4(), av::MediaType::video(), &settings)?;
        writer.nominal_duration = Some(cm::Time::new(1, fps.max(1) as i32));
        Ok(writer)
    }

    /// AAC output, always resampled to 48 kHz: the encoder rejects rates above 48 kHz
    /// and, at our bitrate, low rates such as 8–24 kHz Bluetooth microphones.
    pub fn aac(path: &Path, channels: u32) -> Result<Self> {
        let channels = channels.clamp(1, 2);
        let sample_rate = 48_000.0;
        let mut settings = ns::DictionaryMut::<ns::String, ns::Id>::with_capacity(4);
        settings.insert(
            av::audio::all_formats_keys::id(),
            ns::Number::with_u32(u32::from_be_bytes(*b"aac ")).as_id_ref(),
        );
        settings.insert(
            av::audio::all_formats_keys::sample_rate(),
            ns::Number::with_f64(sample_rate).as_id_ref(),
        );
        settings.insert(
            av::audio::all_formats_keys::number_of_channels(),
            ns::Number::with_u32(channels).as_id_ref(),
        );
        settings.insert(
            ns::str!(c"AVEncoderBitRateKey"),
            ns::Number::with_u32(96_000 * channels).as_id_ref(),
        );
        Self::new(path, av::FileType::m4a(), av::MediaType::audio(), &settings)
    }

    fn new(
        path: &Path,
        file_type: &av::FileType,
        media_type: &av::MediaType,
        settings: &ns::Dictionary<ns::String, ns::Id>,
    ) -> Result<Self> {
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| Error::Platform(e.to_string()))?;
        }
        let url = ns::Url::with_fs_path_str(&path.to_string_lossy(), false);
        let mut writer = av::AssetWriter::with_url_and_file_type(&url, file_type)
            .map_err(|e| platform("cannot create writer", e))?;
        writer.set_should_optimize_for_network_use(false);
        writer.set_movie_fragment_interval(cm::Time::with_secs(FRAGMENT_SECONDS, 600));

        let mut input =
            av::AssetWriterInput::with_media_type_and_output_settings(media_type, Some(settings))
                .map_err(|e| Error::Platform(format!("invalid output settings: {e:?}")))?;
        input.set_expects_media_data_in_real_time(true);
        writer
            .add_input(&input)
            .map_err(|e| Error::Platform(format!("cannot add writer input: {e:?}")))?;
        if !writer.start_writing() {
            return Err(writer_error(&writer, "cannot start writing"));
        }
        Ok(Self {
            writer,
            input,
            started: false,
            end: None,
            last_frame: None,
            nominal_duration: None,
            dropped: 0,
        })
    }

    /// Appends a sample; returns its presentation time if it opened the writing session.
    pub fn append(&mut self, buf: &cm::SampleBuf) -> Result<Option<cm::Time>> {
        let pts = buf.pts();
        let mut opened = None;
        if !self.started {
            self.writer.start_session_at_src_time(pts);
            self.started = true;
            opened = Some(pts);
            log::debug!("first sample: pts {:?}, duration {:?}", pts, buf.duration());
        }
        if !self.input.is_ready_for_more_media_data() {
            self.dropped += 1;
            return Ok(opened);
        }
        match self.input.append_sample_buf(buf) {
            Ok(true) => {}
            Ok(false) => return Err(writer_error(&self.writer, "cannot append sample")),
            Err(e) => return Err(Error::Platform(format!("cannot append sample: {e:?}"))),
        }
        let duration = buf.duration();
        let end = if duration.is_valid() && duration.value > 0 {
            pts.add(duration)
        } else {
            self.nominal_duration.map_or(pts, |d| pts.add(d))
        };
        if self.end.is_none_or(|e| end > e) {
            self.end = Some(end);
            if buf.image_buf().is_some() {
                self.last_frame = Some(buf.retained());
            }
        }
        Ok(opened)
    }

    /// Appends the last frame again so it covers `from..to`; readers that ignore edit
    /// lists then still see the full length.
    fn hold_last_frame(&mut self, from: cm::Time, to: cm::Time) -> Result<()> {
        if to <= from {
            return Ok(());
        }
        let Some(last) = self.last_frame.take() else {
            return Ok(());
        };
        let (Some(image), Some(desc)) = (last.image_buf(), last.format_desc()) else {
            return Ok(());
        };
        let timing = cm::SampleTimingInfo {
            duration: to.sub(from),
            pts: from,
            dts: cm::Time::invalid(),
        };
        let held =
            cm::SampleBuf::with_image_buf(image, true, None, std::ptr::null(), desc, &timing)
                .map_err(|e| Error::Platform(format!("cannot repeat last frame: {e:?}")))?;
        for _ in 0..100 {
            if self.input.is_ready_for_more_media_data() {
                self.append(&held)?;
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    pub fn is_ready(&self) -> bool {
        self.input.is_ready_for_more_media_data()
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Finishes the file. Returns `false` (and removes the file) when nothing was written.
    pub fn finish(self) -> Result<bool> {
        self.finish_at(None)
    }

    /// Like `finish`, but keeps the last sample on screen until `stop` when that is later.
    /// Screen capture only delivers frames on changes, so the last frame can be long before Stop.
    pub fn finish_at(mut self, stop: Option<cm::Time>) -> Result<bool> {
        if !self.started {
            self.writer.cancel_writing();
            return Ok(false);
        }
        if let (Some(end), Some(stop)) = (self.end, stop)
            && stop > end
        {
            self.hold_last_frame(end, stop)?;
        }
        self.input.mark_as_finished();
        if let Some(end) = self.end
            && let Err(e) = self.writer.end_session_at_src_time(end)
        {
            return Err(Error::Platform(format!("cannot end session: {e:?}")));
        }
        let (tx, rx) = mpsc::channel();
        self.writer.finish_writing_with_ch(move || {
            let _ = tx.send(());
        });
        rx.recv_timeout(Duration::from_secs(30))
            .map_err(|_| Error::Platform("timed out finishing the file".into()))?;
        if self.writer.status() != av::AssetWriterStatus::Completed {
            return Err(writer_error(&self.writer, "cannot finish writing"));
        }
        Ok(true)
    }
}

fn writer_error(writer: &av::AssetWriter, context: &str) -> Error {
    match writer.error() {
        Some(err) => Error::Platform(format!("{context}: {}", err.localized_desc())),
        None => Error::Platform(context.to_string()),
    }
}

pub(crate) fn platform(context: &str, err: &ns::Error) -> Error {
    Error::Platform(format!("{context}: {}", err.localized_desc()))
}

#[cfg(test)]
mod tests {
    use std::{thread::sleep, time::Duration};

    use cidre::cm;
    use recast_project::mp4::{self, TrackKind};

    use super::MediaWriter;
    use crate::macos::synthetic;

    fn wait_ready(writer: &MediaWriter) {
        while !writer.is_ready() {
            sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn writes_fragmented_hevc() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("screen.mp4");
        let mut writer = MediaWriter::hevc(&path, 320, 240, 60).unwrap();
        let start = cm::Clock::host_time_clock().time();
        let frame = cm::Time::new(1, 60);
        for i in 0..300 {
            let pts = start.add(cm::Time::new(i, 60));
            let buf = synthetic::video_frame(320, 240, i as u64, pts, frame).unwrap();
            wait_ready(&writer);
            let opened = writer.append(&buf).unwrap();
            assert_eq!(opened.is_some(), i == 0);
        }
        assert_eq!(writer.dropped(), 0);
        assert!(writer.finish().unwrap());

        let summary = mp4::scan_file(&path).unwrap();
        let video = summary.track(TrackKind::Video).unwrap();
        assert!((video.duration_ms() - 5000.0).abs() < 20.0, "{summary:?}");
        assert_eq!(summary.complete_len, summary.file_len);
    }

    #[test]
    fn writes_aac() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("system-audio.m4a");
        let mut writer = MediaWriter::aac(&path, 2).unwrap();
        let start = cm::Clock::host_time_clock().time();
        let frames = 1024;
        for i in 0..141u64 {
            let first = i * frames as u64;
            let pts = start.add(cm::Time::new(first as i64, 48_000));
            let buf = synthetic::audio_chunk(48_000.0, first, frames, pts).unwrap();
            wait_ready(&writer);
            writer.append(&buf).unwrap();
        }
        assert!(writer.finish().unwrap());
        let summary = mp4::scan_file(&path).unwrap();
        let audio = summary.track(TrackKind::Audio).unwrap();
        assert!(
            (3008.0..3100.0).contains(&audio.duration_ms()),
            "{summary:?}"
        );
    }

    #[test]
    fn finish_at_extends_to_stop_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("idle.mp4");
        let mut writer = MediaWriter::hevc(&path, 320, 240, 60).unwrap();
        let start = cm::Clock::host_time_clock().time();
        for i in 0..2 {
            let pts = start.add(cm::Time::new(i, 60));
            let buf =
                synthetic::video_frame(320, 240, i as u64, pts, cm::Time::new(1, 60)).unwrap();
            wait_ready(&writer);
            writer.append(&buf).unwrap();
        }
        let stop = start.add(cm::Time::new(3, 1));
        assert!(writer.finish_at(Some(stop)).unwrap());
        let summary = mp4::scan_file(&path).unwrap();
        let video = summary.track(TrackKind::Video).unwrap();
        assert!((video.duration_ms() - 3000.0).abs() < 20.0, "{summary:?}");
    }

    fn frames_without_duration(count: i64, stop_after: cm::Time) -> f64 {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no-duration.mp4");
        let mut writer = MediaWriter::hevc(&path, 320, 240, 60).unwrap();
        let start = cm::Clock::host_time_clock().time();
        for i in 0..count {
            let pts = start.add(cm::Time::new(i, 60));
            let buf = synthetic::video_frame(320, 240, i as u64, pts, cm::Time::invalid()).unwrap();
            assert!(!buf.duration().is_valid());
            wait_ready(&writer);
            writer.append(&buf).unwrap();
        }
        let last = start.add(cm::Time::new(count - 1, 60));
        assert!(writer.finish_at(Some(last.add(stop_after))).unwrap());
        let summary = mp4::scan_file(&path).unwrap();
        summary.track(TrackKind::Video).unwrap().duration_ms()
    }

    #[test]
    fn frames_without_duration_hold_until_stop() {
        let ms = frames_without_duration(300, cm::Time::new(3, 1));
        assert!((ms - 7983.0).abs() < 20.0, "{ms}");
        let ms = frames_without_duration(1, cm::Time::new(2, 1));
        assert!((ms - 2000.0).abs() < 20.0, "{ms}");
        let ms = frames_without_duration(5, cm::Time::new(0, 1));
        assert!((ms - 83.3).abs() < 20.0, "{ms}");
    }

    #[test]
    fn aac_caps_high_sample_rates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mic.m4a");
        let mut writer = MediaWriter::aac(&path, 1).unwrap();
        let start = cm::Clock::host_time_clock().time();
        for i in 0..94u64 {
            let first = i * 1024;
            let pts = start.add(cm::Time::new(first as i64, 96_000));
            let buf = synthetic::audio_chunk(96_000.0, first, 1024, pts).unwrap();
            wait_ready(&writer);
            writer.append(&buf).unwrap();
        }
        assert!(writer.finish().unwrap());
        let summary = mp4::scan_file(&path).unwrap();
        let audio = summary.track(TrackKind::Audio).unwrap();
        assert_eq!(audio.timescale, 48_000);
        assert!(
            (990.0..1100.0).contains(&audio.duration_ms()),
            "{summary:?}"
        );
    }

    #[test]
    fn aac_accepts_low_sample_rates() {
        for rate in [8_000, 16_000, 22_050, 24_000] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("mic.m4a");
            let mut writer = MediaWriter::aac(&path, 1).unwrap();
            let start = cm::Clock::host_time_clock().time();
            let chunks = rate / 1024 + 1;
            for i in 0..chunks as u64 {
                let first = i * 1024;
                let pts = start.add(cm::Time::new(first as i64, rate));
                let buf = synthetic::audio_chunk(rate as f64, first, 1024, pts).unwrap();
                wait_ready(&writer);
                writer
                    .append(&buf)
                    .unwrap_or_else(|e| panic!("{rate} Hz: {e}"));
            }
            assert!(writer.finish().unwrap(), "{rate} Hz");
            let summary = mp4::scan_file(&path).unwrap();
            let audio = summary.track(TrackKind::Audio).unwrap();
            assert_eq!(audio.timescale, 48_000, "{rate} Hz");
            assert!(
                (950.0..1300.0).contains(&audio.duration_ms()),
                "{rate} Hz: {summary:?}"
            );
        }
    }

    #[test]
    fn finish_without_samples_reports_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mic.m4a");
        let writer = MediaWriter::aac(&path, 1).unwrap();
        assert!(!writer.finish().unwrap());
    }
}
