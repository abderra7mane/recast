use std::{collections::VecDeque, ffi::c_void, path::Path, sync::mpsc, time::Duration};

use cidre::{arc, av, cat, cm, ns, objc::Obj};

use crate::{Error, Result, video_bitrate};

const FRAGMENT_SECONDS: f64 = 2.0;
/// Audio buffers waiting for the writer; beyond this the oldest are dropped and
/// their time is filled with silence.
const MAX_QUEUED_AUDIO: usize = 500;
/// Gaps between audio buffers longer than this are filled with silence.
const AUDIO_GAP_SECONDS: f64 = 0.002;

unsafe extern "C" {
    fn CMAudioSampleBufferCreateReadyWithPacketDescriptions(
        allocator: *const c_void,
        data_buffer: &cm::BlockBuf,
        format_description: &cm::FormatDesc,
        num_samples: cm::ItemCount,
        pts: cm::Time,
        packet_descriptions: *const c_void,
        sample_buffer_out: *mut Option<arc::R<cm::SampleBuf>>,
    ) -> i32;
}

/// AAC drops the timing of its input, so audio is written without holes: buffers
/// wait here instead of being dropped, and gaps are filled with silence.
#[derive(Default)]
struct AudioQueue {
    pending: VecDeque<arc::R<cm::SampleBuf>>,
    /// Where the audio written so far ends.
    next: Option<cm::Time>,
}

fn audio_end(buf: &cm::SampleBuf) -> cm::Time {
    let rate = buf
        .format_desc()
        .and_then(|d| d.stream_basic_desc())
        .map_or(0.0, |asbd| asbd.sample_rate);
    if rate > 0.0 {
        buf.pts()
            .add(cm::Time::new(buf.num_samples() as i64, rate.round() as i32))
    } else {
        buf.pts().add(buf.duration())
    }
}

/// Silent linear PCM in the format of `like`, from `from` until `to` or for at most
/// one second. `None` for formats that cannot be filled with zeros.
fn silence(
    like: &cm::SampleBuf,
    from: cm::Time,
    to: cm::Time,
) -> Result<Option<arc::R<cm::SampleBuf>>> {
    let Some(desc) = like.format_desc() else {
        return Ok(None);
    };
    let Some(asbd) = desc.stream_basic_desc() else {
        return Ok(None);
    };
    if asbd.format != cat::AudioFormat::LINEAR_PCM || asbd.bytes_per_frame == 0 {
        return Ok(None);
    }
    let rate = asbd.sample_rate;
    let frames = ((to.sub(from).as_secs() * rate).round() as usize).min(rate as usize);
    if frames == 0 {
        return Ok(None);
    }
    let planes = if asbd
        .format_flags
        .contains(cat::AudioFormatFlags::IS_NON_INTERLEAVED)
    {
        asbd.channels_per_frame.max(1) as usize
    } else {
        1
    };
    let fail = |e| Error::Platform(format!("cannot create silence: {e:?}"));
    let mut block = cm::BlockBuf::with_mem_block(frames * asbd.bytes_per_frame as usize * planes)
        .map_err(fail)?;
    block.assure_block_mem().map_err(fail)?;
    block.as_mut_slice().map_err(fail)?.fill(0);
    let mut out = None;
    // SAFETY: linear PCM needs no packet descriptions; the block holds `frames` frames.
    let status = unsafe {
        CMAudioSampleBufferCreateReadyWithPacketDescriptions(
            std::ptr::null(),
            &block,
            desc,
            frames as cm::ItemCount,
            from,
            std::ptr::null(),
            &mut out,
        )
    };
    if status != 0 {
        return Err(Error::Platform(format!("cannot create silence: {status}")));
    }
    Ok(out)
}

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
    audio: Option<AudioQueue>,
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
        let mut writer = Self::new(path, av::FileType::m4a(), av::MediaType::audio(), &settings)?;
        writer.audio = Some(AudioQueue::default());
        Ok(writer)
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
            audio: None,
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
        if let Some(audio) = self.audio.as_mut() {
            audio.pending.push_back(buf.retained());
            if audio.pending.len() > MAX_QUEUED_AUDIO {
                audio.pending.pop_front();
                self.dropped += 1;
            }
            self.flush_audio()?;
            return Ok(opened);
        }
        if !self.input.is_ready_for_more_media_data() {
            self.dropped += 1;
            return Ok(opened);
        }
        self.write(buf)?;
        Ok(opened)
    }

    fn write(&mut self, buf: &cm::SampleBuf) -> Result<()> {
        let pts = buf.pts();
        match self.input.append_sample_buf(buf) {
            Ok(true) => {}
            Ok(false) => return Err(writer_error(&self.writer, "cannot append sample")),
            Err(e) => return Err(Error::Platform(format!("cannot append sample: {e:?}"))),
        }
        let duration = buf.duration();
        let end = if self.audio.is_some() {
            audio_end(buf)
        } else if duration.is_valid() && duration.value > 0 {
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
        Ok(())
    }

    /// Writes queued audio while the writer takes it, filling gaps with silence.
    fn flush_audio(&mut self) -> Result<()> {
        while self.input.is_ready_for_more_media_data() {
            let Some(audio) = self.audio.as_mut() else {
                return Ok(());
            };
            let Some(front) = audio.pending.front().cloned() else {
                return Ok(());
            };
            if let Some(next) = audio.next
                && front.pts().sub(next).as_secs() > AUDIO_GAP_SECONDS
                && let Some(gap) = silence(&front, next, front.pts())?
            {
                let end = audio_end(&gap);
                self.write(&gap)?;
                self.audio.as_mut().expect("audio queue").next = Some(end);
                continue;
            }
            audio.pending.pop_front();
            audio.next = Some(audio_end(&front));
            self.write(&front)?;
        }
        Ok(())
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
        for _ in 0..500 {
            self.flush_audio()?;
            if self.audio.as_ref().is_none_or(|a| a.pending.is_empty()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
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

    /// Decodes an audio file to interleaved stereo float at 48 kHz.
    fn decode_audio(path: &std::path::Path) -> Vec<f32> {
        use cidre::{av, ns};
        let url = ns::Url::with_fs_path_str(&path.to_string_lossy(), false);
        let asset = av::UrlAsset::with_url(&url, None).unwrap();
        let tracks =
            crate::macos::block_on(asset.load_tracks_with_media_type(av::MediaType::audio()))
                .unwrap();
        let track = tracks.get(0).unwrap();
        use av::audio::{all_formats_keys as all, linear_pcm_keys as pcm};
        let mut settings = ns::DictionaryMut::<ns::String, ns::Id>::with_capacity(6);
        settings.insert(
            all::id(),
            ns::Number::with_u32(u32::from_be_bytes(*b"lpcm")).as_id_ref(),
        );
        settings.insert(
            all::sample_rate(),
            ns::Number::with_f64(48_000.0).as_id_ref(),
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
        let mut reader = av::AssetReader::with_asset(&asset).unwrap();
        let mut output = av::AssetReaderTrackOutput::with_track(&track, Some(&settings)).unwrap();
        reader.add_output(&output).unwrap();
        assert!(reader.start_reading().unwrap());
        let mut samples = Vec::new();
        while let Some(buf) = output.next_sample_buf().unwrap() {
            let block = buf.data_buf().unwrap();
            let mut bytes = vec![0u8; block.data_len()];
            block.copy_to(0, &mut bytes).unwrap();
            samples.extend(
                bytes
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            );
        }
        samples
    }

    fn rms(samples: &[f32], from_s: f64, to_s: f64) -> f32 {
        let range = &samples[(from_s * 96_000.0) as usize..(to_s * 96_000.0) as usize];
        (range.iter().map(|s| s * s).sum::<f32>() / range.len() as f32).sqrt()
    }

    /// Non-interleaved float stereo, the layout ScreenCaptureKit delivers.
    fn planar_chunk(first: u64, frames: usize, pts: cm::Time) -> cidre::arc::R<cm::SampleBuf> {
        use cidre::cat;
        let asbd = cat::audio::StreamBasicDesc {
            sample_rate: 48_000.0,
            format: cat::AudioFormat::LINEAR_PCM,
            format_flags: cat::AudioFormatFlags::IS_FLOAT
                | cat::AudioFormatFlags::IS_PACKED
                | cat::AudioFormatFlags::IS_NON_INTERLEAVED,
            bytes_per_packet: 4,
            frames_per_packet: 1,
            bytes_per_frame: 4,
            channels_per_frame: 2,
            bits_per_channel: 32,
            reserved: 0,
        };
        let desc = cm::AudioFormatDesc::with_asbd(&asbd).unwrap();
        let mut block = cm::BlockBuf::with_mem_block(frames * 8).unwrap();
        block.assure_block_mem().unwrap();
        let bytes = block.as_mut_slice().unwrap();
        for plane in 0..2 {
            for i in 0..frames {
                let t = (first + i as u64) as f64 / 48_000.0;
                let v = ((t * 440.0 * std::f64::consts::TAU).sin() * 0.2) as f32;
                let at = (plane * frames + i) * 4;
                bytes[at..at + 4].copy_from_slice(&v.to_ne_bytes());
            }
        }
        let mut out = None;
        // SAFETY: linear PCM needs no packet descriptions; the block holds `frames` frames.
        let status = unsafe {
            super::CMAudioSampleBufferCreateReadyWithPacketDescriptions(
                std::ptr::null(),
                &block,
                &desc,
                frames as cm::ItemCount,
                pts,
                std::ptr::null(),
                &mut out,
            )
        };
        assert_eq!(status, 0);
        out.unwrap()
    }

    fn tone_with_gap(planar: bool) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("system-audio.m4a");
        let mut writer = MediaWriter::aac(&path, 2).unwrap();
        let start = cm::Time::new(5_000, 1);
        for i in 0..(3 * 48_000 / 1024) as u64 {
            let first = i * 1024;
            if (48_000..96_000).contains(&first) {
                continue;
            }
            let pts = start.add(cm::Time::new(first as i64, 48_000));
            let buf = if planar {
                planar_chunk(first, 1024, pts)
            } else {
                synthetic::audio_chunk(48_000.0, first, 1024, pts).unwrap()
            };
            wait_ready(&writer);
            writer.append(&buf).unwrap();
        }
        assert!(writer.finish().unwrap());

        let samples = decode_audio(&path);
        let seconds = samples.len() as f64 / 96_000.0;
        assert!((seconds - 3.0).abs() < 0.05, "{seconds} s");
        assert!(rms(&samples, 0.2, 0.8) > 0.1);
        assert!(rms(&samples, 1.1, 1.9) < 0.005);
        assert!(rms(&samples, 2.2, 2.8) > 0.1);
        let onset = samples[(1.5 * 96_000.0) as usize..]
            .iter()
            .position(|s| s.abs() > 0.05)
            .map(|i| 1.5 + i as f64 / 96_000.0)
            .unwrap();
        assert!((onset - 2.0).abs() < 0.03, "tone resumes at {onset} s");
    }

    #[test]
    fn audio_gaps_become_silence() {
        tone_with_gap(false);
    }

    #[test]
    fn planar_audio_gaps_become_silence() {
        tone_with_gap(true);
    }

    #[test]
    fn finish_without_samples_reports_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mic.m4a");
        let writer = MediaWriter::aac(&path, 1).unwrap();
        assert!(!writer.finish().unwrap());
    }
}
