use std::{path::Path, sync::mpsc, time::Duration};

use cidre::{arc, av, cat, cm, cv, ns, objc::Obj};
use recast_project::Codec;

use crate::{Error, Result, SAMPLE_RATE};

/// Video bitrate for `quality` 0..1, in bits per second.
pub fn bitrate(width: u32, height: u32, fps: u32, codec: Codec, quality: f64) -> u32 {
    let bits_per_pixel = 0.05 + 0.25 * quality.clamp(0.0, 1.0).powi(2);
    let efficiency = match codec {
        Codec::H264 => 1.0,
        Codec::Hevc => 0.65,
    };
    let bits = width as f64 * height as f64 * fps as f64 * bits_per_pixel * efficiency;
    bits.clamp(2_000_000.0, 200_000_000.0) as u32
}

fn writer_error(writer: &av::AssetWriter, context: &str) -> Error {
    match writer.error() {
        Some(e) => Error::Media(format!("{context}: {}", e.localized_desc())),
        None => Error::Media(context.to_string()),
    }
}

pub struct EncoderOptions {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub codec: Codec,
    pub bitrate: u32,
    pub audio: bool,
}

/// Writes BGRA frames and stereo float audio to an MP4 with AVAssetWriter.
pub struct Encoder {
    writer: arc::R<av::AssetWriter>,
    video: arc::R<av::AssetWriterInput>,
    adaptor: arc::R<av::asset::WriterInputPixelBufAdaptor>,
    audio: Option<(arc::R<av::AssetWriterInput>, arc::R<cm::AudioFormatDesc>)>,
    fps: u32,
    width: u32,
    height: u32,
}

impl Encoder {
    pub fn create(path: &Path, options: &EncoderOptions) -> Result<Self> {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let url = ns::Url::with_fs_path_str(&path.to_string_lossy(), false);
        let mut writer = av::AssetWriter::with_url_and_file_type(&url, av::FileType::mp4())
            .map_err(|e| {
                Error::Media(format!(
                    "cannot create {}: {}",
                    path.display(),
                    e.localized_desc()
                ))
            })?;
        writer.set_should_optimize_for_network_use(true);

        let mut video = av::AssetWriterInput::with_media_type_and_output_settings(
            av::MediaType::video(),
            Some(&video_settings(options)),
        )
        .map_err(|e| Error::Media(format!("invalid video settings: {e:?}")))?;
        video.set_expects_media_data_in_real_time(false);
        writer
            .add_input(&video)
            .map_err(|e| Error::Media(format!("cannot add video: {e:?}")))?;

        let mut attrs = ns::DictionaryMut::<ns::String, ns::Id>::with_capacity(3);
        attrs.insert(
            cv::pixel_buffer_keys::pixel_format().as_ns(),
            ns::Number::with_u32(cv::PixelFormat::_32_BGRA.0).as_id_ref(),
        );
        attrs.insert(
            cv::pixel_buffer_keys::width().as_ns(),
            ns::Number::with_u32(options.width).as_id_ref(),
        );
        attrs.insert(
            cv::pixel_buffer_keys::height().as_ns(),
            ns::Number::with_u32(options.height).as_id_ref(),
        );
        let adaptor =
            av::asset::WriterInputPixelBufAdaptor::with_input_writer(&video, Some(&attrs))
                .map_err(|e| Error::Media(format!("cannot create the frame adaptor: {e:?}")))?;

        let audio = if options.audio {
            let mut input = av::AssetWriterInput::with_media_type_and_output_settings(
                av::MediaType::audio(),
                Some(&audio_settings()),
            )
            .map_err(|e| Error::Media(format!("invalid audio settings: {e:?}")))?;
            input.set_expects_media_data_in_real_time(false);
            writer
                .add_input(&input)
                .map_err(|e| Error::Media(format!("cannot add audio: {e:?}")))?;
            let asbd = cat::audio::StreamBasicDesc {
                sample_rate: SAMPLE_RATE as f64,
                format: cat::AudioFormat::LINEAR_PCM,
                format_flags: cat::AudioFormatFlags::IS_FLOAT | cat::AudioFormatFlags::IS_PACKED,
                bytes_per_packet: 8,
                frames_per_packet: 1,
                bytes_per_frame: 8,
                channels_per_frame: 2,
                bits_per_channel: 32,
                reserved: 0,
            };
            let format = cm::AudioFormatDesc::with_asbd(&asbd)
                .map_err(|e| Error::Media(format!("audio format: {e:?}")))?;
            Some((input, format))
        } else {
            None
        };

        if !writer.start_writing() {
            return Err(writer_error(&writer, "cannot start writing"));
        }
        writer.start_session_at_src_time(cm::Time::zero());
        Ok(Self {
            writer,
            video,
            adaptor,
            audio,
            fps: options.fps,
            width: options.width,
            height: options.height,
        })
    }

    pub fn video_ready(&self) -> bool {
        self.video.is_ready_for_more_media_data()
    }

    pub fn audio_ready(&self) -> bool {
        self.audio
            .as_ref()
            .is_some_and(|(input, _)| input.is_ready_for_more_media_data())
    }

    fn check_failed(&self) -> Result<()> {
        if self.writer.status() == av::AssetWriterStatus::Failed {
            return Err(writer_error(&self.writer, "encoding failed"));
        }
        Ok(())
    }

    /// Appends frame `index`; `fill` writes BGRA rows into a buffer whose rows are
    /// the given number of bytes apart.
    pub fn append_frame(&mut self, index: u64, fill: impl FnOnce(&mut [u8], usize)) -> Result<()> {
        self.check_failed()?;
        let pool = self
            .adaptor
            .pixel_buf_pool()
            .ok_or_else(|| writer_error(&self.writer, "no pixel buffer pool"))?;
        let mut buffer = pool
            .pixel_buf()
            .map_err(|e| Error::Media(format!("cannot get a pixel buffer: {e:?}")))?;
        if buffer.width() != self.width as usize || buffer.height() != self.height as usize {
            return Err(Error::Media("pixel buffer has the wrong size".into()));
        }
        // SAFETY: unlocked right after writing; the buffer holds `stride * height` bytes.
        unsafe {
            buffer
                .lock_base_addr(Default::default())
                .result()
                .map_err(|e| Error::Media(format!("cannot lock a pixel buffer: {e:?}")))?;
            let stride = buffer.bytes_per_row();
            let data = std::slice::from_raw_parts_mut(
                buffer.base_address_mut() as *mut u8,
                stride * self.height as usize,
            );
            fill(data, stride);
            let _ = buffer.unlock_lock_base_addr(Default::default());
        }
        let pts = cm::Time::new(index as i64, self.fps as i32);
        match self.adaptor.append_pixel_buf_with_pts(&buffer, pts) {
            Ok(true) => Ok(()),
            Ok(false) => Err(writer_error(&self.writer, "cannot append a frame")),
            Err(e) => Err(Error::Media(format!("cannot append a frame: {e:?}"))),
        }
    }

    /// Appends interleaved stereo samples starting at sample frame `first`.
    pub fn append_audio(&mut self, first: u64, samples: &[f32]) -> Result<()> {
        self.check_failed()?;
        let Some((input, format)) = self.audio.as_mut() else {
            return Ok(());
        };
        let frames = samples.len() / 2;
        if frames == 0 {
            return Ok(());
        }
        let mut block = cm::BlockBuf::with_mem_block(frames * 8)
            .map_err(|e| Error::Media(format!("audio block: {e:?}")))?;
        block
            .assure_block_mem()
            .map_err(|e| Error::Media(format!("audio block: {e:?}")))?;
        {
            let bytes = block
                .as_mut_slice()
                .map_err(|e| Error::Media(format!("audio block: {e:?}")))?;
            for (dst, s) in bytes.chunks_exact_mut(4).zip(&samples[..frames * 2]) {
                dst.copy_from_slice(&s.to_le_bytes());
            }
        }
        let timing = cm::SampleTimingInfo {
            duration: cm::Time::new(1, SAMPLE_RATE as i32),
            pts: cm::Time::new(first as i64, SAMPLE_RATE as i32),
            dts: cm::Time::invalid(),
        };
        let mut sample = None;
        // SAFETY: one timing entry and one size entry describe `frames` packed frames.
        unsafe {
            cm::SampleBuf::create_in(
                None,
                Some(&block),
                true,
                None,
                std::ptr::null(),
                Some(format),
                frames as _,
                1,
                &timing,
                1,
                &8usize,
                &mut sample,
            )
        }
        .map_err(|e| Error::Media(format!("audio sample: {e:?}")))?;
        let sample = sample.ok_or_else(|| Error::Media("audio sample".into()))?;
        match input.append_sample_buf(&sample) {
            Ok(true) => Ok(()),
            Ok(false) => Err(writer_error(&self.writer, "cannot append audio")),
            Err(e) => Err(Error::Media(format!("cannot append audio: {e:?}"))),
        }
    }

    pub fn finish_audio(&mut self) {
        if let Some((input, _)) = self.audio.as_mut() {
            input.mark_as_finished();
        }
    }

    pub fn finish(mut self, end_frame: u64) -> Result<()> {
        self.video.mark_as_finished();
        self.finish_audio();
        self.writer
            .end_session_at_src_time(cm::Time::new(end_frame as i64, self.fps as i32))
            .map_err(|e| Error::Media(format!("cannot end the session: {e:?}")))?;
        let (tx, rx) = mpsc::channel();
        self.writer.finish_writing_with_ch(move || {
            let _ = tx.send(());
        });
        rx.recv_timeout(Duration::from_secs(300))
            .map_err(|_| Error::Media("timed out finishing the file".into()))?;
        if self.writer.status() != av::AssetWriterStatus::Completed {
            return Err(writer_error(&self.writer, "cannot finish writing"));
        }
        Ok(())
    }

    pub fn cancel(mut self) {
        self.writer.cancel_writing();
    }
}

fn video_settings(options: &EncoderOptions) -> arc::R<ns::DictionaryMut<ns::String, ns::Id>> {
    let mut compression = ns::DictionaryMut::<ns::String, ns::Id>::with_capacity(4);
    compression.insert(
        ns::str!(c"AverageBitRate"),
        ns::Number::with_u32(options.bitrate).as_id_ref(),
    );
    compression.insert(
        ns::str!(c"ExpectedFrameRate"),
        ns::Number::with_u32(options.fps).as_id_ref(),
    );
    compression.insert(
        ns::str!(c"MaxKeyFrameIntervalDuration"),
        ns::Number::with_f64(2.0).as_id_ref(),
    );
    let profile = match options.codec {
        Codec::H264 => ns::str!(c"H264_High_AutoLevel"),
        Codec::Hevc => ns::str!(c"HEVC_Main_AutoLevel"),
    };
    compression.insert(ns::str!(c"ProfileLevel"), profile.as_id_ref());

    let mut color = ns::DictionaryMut::<ns::String, ns::Id>::with_capacity(3);
    for key in [
        ns::str!(c"ColorPrimaries"),
        ns::str!(c"TransferFunction"),
        ns::str!(c"YCbCrMatrix"),
    ] {
        color.insert(key, ns::str!(c"ITU_R_709_2").as_id_ref());
    }

    use av::video_settings_keys as keys;
    let codec = match options.codec {
        Codec::H264 => av::VideoCodec::h264(),
        Codec::Hevc => av::VideoCodec::hevc(),
    };
    let mut settings = ns::DictionaryMut::<ns::String, ns::Id>::with_capacity(5);
    settings.insert(keys::codec(), codec.as_id_ref());
    settings.insert(
        keys::width(),
        ns::Number::with_u32(options.width).as_id_ref(),
    );
    settings.insert(
        keys::height(),
        ns::Number::with_u32(options.height).as_id_ref(),
    );
    settings.insert(keys::compression_props(), compression.as_id_ref());
    settings.insert(keys::color_props(), color.as_id_ref());
    settings
}

fn audio_settings() -> arc::R<ns::DictionaryMut<ns::String, ns::Id>> {
    use av::audio::all_formats_keys as keys;
    let mut settings = ns::DictionaryMut::<ns::String, ns::Id>::with_capacity(4);
    settings.insert(
        keys::id(),
        ns::Number::with_u32(u32::from_be_bytes(*b"aac ")).as_id_ref(),
    );
    settings.insert(
        keys::sample_rate(),
        ns::Number::with_f64(SAMPLE_RATE as f64).as_id_ref(),
    );
    settings.insert(
        keys::number_of_channels(),
        ns::Number::with_u32(2).as_id_ref(),
    );
    settings.insert(
        ns::str!(c"AVEncoderBitRateKey"),
        ns::Number::with_u32(192_000).as_id_ref(),
    );
    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitrate_grows_with_quality_and_size() {
        let low = bitrate(1920, 1080, 60, Codec::H264, 0.0);
        let mid = bitrate(1920, 1080, 60, Codec::H264, 0.7);
        let high = bitrate(1920, 1080, 60, Codec::H264, 1.0);
        assert!(low < mid && mid < high);
        assert!((15_000_000..30_000_000).contains(&mid), "{mid}");
        assert!(bitrate(1920, 1080, 60, Codec::Hevc, 0.7) < mid);
        assert!(bitrate(3840, 2160, 60, Codec::H264, 0.7) > mid);
        assert_eq!(bitrate(16, 16, 30, Codec::H264, 0.0), 2_000_000);
    }
}
