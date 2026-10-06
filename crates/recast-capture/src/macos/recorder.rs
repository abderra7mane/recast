use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use cidre::{
    arc, cg, cm, cv, define_obj_type, dispatch, ns, objc,
    sc::{self, StreamDelegate, StreamOutput},
};

use super::{
    block_on, content, host_time_ns,
    mic::MicCapture,
    writer::{MediaWriter, platform},
};
use crate::{
    ActiveCapture, CaptureEvent, CaptureInfo, CaptureOptions, Error, EventHandler, OutputFiles,
    Rect, Result, Track, video_area,
};

/// Writes one audio track, creating the file once the first buffer reveals the format.
pub(super) struct AudioSlot {
    path: PathBuf,
    writer: Option<MediaWriter>,
    failed: bool,
}

impl AudioSlot {
    pub(super) fn new(path: PathBuf) -> Self {
        Self {
            path,
            writer: None,
            failed: false,
        }
    }

    pub(super) fn append(
        &mut self,
        buf: &cm::SampleBuf,
        track: Track,
        to_host_ns: impl Fn(cm::Time) -> u64,
        on_event: &EventHandler,
    ) {
        if self.failed {
            return;
        }
        if let Err(e) = self.try_append(buf, track, to_host_ns, on_event) {
            self.failed = true;
            on_event(CaptureEvent::Failed {
                message: format!("{track:?}: {e}"),
            });
        }
    }

    fn try_append(
        &mut self,
        buf: &cm::SampleBuf,
        track: Track,
        to_host_ns: impl Fn(cm::Time) -> u64,
        on_event: &EventHandler,
    ) -> Result<()> {
        let writer = match &mut self.writer {
            Some(w) => w,
            None => {
                let asbd = buf
                    .format_desc()
                    .and_then(|d| d.stream_basic_desc())
                    .ok_or_else(|| Error::Platform("audio buffer without format".into()))?;
                self.writer
                    .insert(MediaWriter::aac(&self.path, asbd.channels_per_frame)?)
            }
        };
        if let Some(pts) = writer.append(buf)? {
            on_event(CaptureEvent::TrackStarted {
                track,
                host_ns: to_host_ns(pts),
            });
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<()> {
        match self.writer {
            Some(w) => w.finish().map(|_| ()),
            None => Ok(()),
        }
    }
}

/// A window recorded through a crop to whole pixels. If the window changes size, the
/// crop would cut it off, so the stream switches to scaling the whole window into the
/// video instead.
struct Refit {
    watch: ResizeWatch,
    width: u32,
    height: u32,
    fps: u32,
    system_audio: bool,
}

/// Reports, once, when a window's size differs from its size at the start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ResizeWatch {
    width: f64,
    height: f64,
    fired: bool,
}

impl ResizeWatch {
    pub(super) fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            fired: false,
        }
    }

    pub(super) fn resized(&mut self, width: f64, height: f64) -> bool {
        let changed = (width - self.width).abs() > 0.5 || (height - self.height).abs() > 0.5;
        if changed && !self.fired {
            self.fired = true;
            return true;
        }
        false
    }
}

/// The window's size in points from a frame's `SCStreamFrameInfoScreenRect`.
fn screen_size(buf: &cm::SampleBuf) -> Option<(f64, f64)> {
    let attachments = buf.attaches(false)?;
    if attachments.is_empty() {
        return None;
    }
    let dict = attachments[0]
        .get(sc::FrameInfo::screen_rect().as_cf())?
        .try_as_dictionary()?;
    let rect = cg::Rect::from_dictionary_representation(dict)?;
    Some((rect.size.width, rect.size.height))
}

struct Shared {
    video: Mutex<Option<MediaWriter>>,
    refit: Mutex<Option<Refit>>,
    video_failed: Mutex<bool>,
    system_audio: Mutex<Option<AudioSlot>>,
    on_event: EventHandler,
}

impl Shared {
    fn on_video(&self, stream: &sc::Stream, buf: &cm::SampleBuf) {
        if buf.image_buf().is_none() || !frame_complete(buf) {
            return;
        }
        self.refit_if_resized(stream, buf);
        let mut guard = self.video.lock().expect("video lock");
        let Some(writer) = guard.as_mut() else {
            return;
        };
        match writer.append(buf) {
            Ok(Some(pts)) => (self.on_event)(CaptureEvent::TrackStarted {
                track: Track::Video,
                host_ns: host_time_ns(pts),
            }),
            Ok(None) => {}
            Err(e) => {
                let mut failed = self.video_failed.lock().expect("video flag");
                if !*failed {
                    *failed = true;
                    (self.on_event)(CaptureEvent::Failed {
                        message: format!("video: {e}"),
                    });
                }
            }
        }
    }

    fn refit_if_resized(&self, stream: &sc::Stream, buf: &cm::SampleBuf) {
        let mut guard = self.refit.lock().expect("refit lock");
        let Some(refit) = guard.as_mut() else {
            return;
        };
        let Some((width, height)) = screen_size(buf) else {
            return;
        };
        if refit.watch.resized(width, height) {
            log::info!("the window was resized, scaling it into the video from now on");
            let cfg = stream_cfg(
                refit.width,
                refit.height,
                refit.fps,
                None,
                refit.system_audio,
            );
            stream.update_cfg_ch(&cfg, |error| {
                if let Some(error) = error {
                    log::warn!("cannot fit the resized window: {}", error.localized_desc());
                }
            });
            *guard = None;
        }
    }

    fn on_audio(&self, buf: &cm::SampleBuf) {
        if let Some(slot) = self.system_audio.lock().expect("audio lock").as_mut() {
            slot.append(buf, Track::SystemAudio, host_time_ns, &self.on_event);
        }
    }
}

fn frame_complete(buf: &cm::SampleBuf) -> bool {
    let Some(attachments) = buf.attaches(false) else {
        return false;
    };
    if attachments.is_empty() {
        return false;
    }
    attachments[0]
        .get(sc::FrameInfo::status().as_cf())
        .and_then(|v| v.try_as_number())
        .and_then(|n| n.to_i64())
        == Some(sc::FrameStatus::Complete as i64)
}

pub(super) struct SinkInner {
    shared: Arc<Shared>,
}

define_obj_type!(
    StreamSink(ns::Id) + sc::StreamOutputImpl + sc::StreamDelegateImpl,
    SinkInner,
    RECAST_STREAM_SINK
);

impl StreamOutput for StreamSink {}
impl StreamDelegate for StreamSink {}

#[objc::add_methods]
impl sc::StreamOutputImpl for StreamSink {
    extern "C" fn impl_stream_did_output_sample_buf(
        &mut self,
        _cmd: Option<&objc::Sel>,
        stream: &sc::Stream,
        sample_buf: &mut cm::SampleBuf,
        kind: sc::OutputType,
    ) {
        let shared = &self.inner().shared;
        match kind {
            sc::OutputType::Screen => shared.on_video(stream, sample_buf),
            sc::OutputType::Audio => shared.on_audio(sample_buf),
            sc::OutputType::Mic => {}
        }
    }
}

#[objc::add_methods]
impl sc::StreamDelegateImpl for StreamSink {
    extern "C" fn impl_stream_did_stop_with_err(
        &mut self,
        _cmd: Option<&objc::Sel>,
        _stream: &sc::Stream,
        error: &ns::Error,
    ) {
        (self.inner().shared.on_event)(CaptureEvent::Failed {
            message: format!("capture stopped: {}", error.localized_desc()),
        });
    }
}

pub(super) struct Recorder {
    info: CaptureInfo,
    stream: arc::R<sc::Stream>,
    sink: arc::R<StreamSink>,
    shared: Arc<Shared>,
    mic: Option<MicCapture>,
    _queues: (arc::R<dispatch::Queue>, arc::R<dispatch::Queue>),
}

// The stream and its sink are thread-safe Objective-C objects; writers are behind locks.
unsafe impl Send for Recorder {}

struct Plan {
    filter: arc::R<sc::ContentFilter>,
    info: CaptureInfo,
    src_rect: Option<cg::Rect>,
    /// A cropped window's size in points, watched for resizes.
    cropped_window: Option<(f64, f64)>,
}

fn stream_cfg(
    width: u32,
    height: u32,
    fps: u32,
    src_rect: Option<cg::Rect>,
    system_audio: bool,
) -> arc::R<sc::StreamCfg> {
    let mut cfg = sc::StreamCfg::new();
    cfg.set_width(width as usize);
    cfg.set_height(height as usize);
    cfg.set_minimum_frame_interval(cm::Time::new(1, fps as i32));
    cfg.set_pixel_format(cv::PixelFormat::_420V);
    cfg.set_shows_cursor(false);
    cfg.set_queue_depth(6);
    cfg.set_capture_resolution(sc::CaptureResolution::Best);
    if let Some(rect) = src_rect {
        cfg.set_src_rect(rect);
    }
    cfg.set_captures_audio(system_audio);
    if system_audio {
        cfg.set_sample_rate(48_000);
        cfg.set_channel_count(2);
        cfg.set_excludes_current_process_audio(true);
    }
    cfg
}

fn plan(content: &sc::ShareableContent, options: &CaptureOptions) -> Result<Plan> {
    let source = content::source(content, &options.target)?;
    let window = source.window.as_ref();
    let requested = source.src_rect.map_or(
        Rect {
            x: 0.0,
            y: 0.0,
            width: source.width,
            height: source.height,
        },
        content::rect,
    );
    let area = video_area(&requested, source.scale);
    let src_rect = (source.src_rect.is_some() || area.rect != requested).then_some(cg::Rect {
        origin: cg::Point {
            x: area.rect.x,
            y: area.rect.y,
        },
        size: cg::Size {
            width: area.rect.width,
            height: area.rect.height,
        },
    });
    Ok(Plan {
        info: CaptureInfo {
            width: area.width,
            height: area.height,
            scale_factor: source.scale,
            bounds: Rect {
                x: source.bounds.x + area.rect.x - requested.x,
                y: source.bounds.y + area.rect.y - requested.y,
                width: area.rect.width,
                height: area.rect.height,
            },
            fps: options.fps.clamp(1, 120),
            codec: "hevc".into(),
            window_title: window.and_then(|w| w.title()).map(|t| t.to_string()),
            app_name: window
                .and_then(|w| w.owning_app())
                .map(|a| a.app_name().to_string()),
        },
        cropped_window: (window.is_some() && src_rect.is_some())
            .then_some((source.width, source.height)),
        filter: source.filter,
        src_rect,
    })
}

impl Recorder {
    pub(super) fn start(
        options: &CaptureOptions,
        files: &OutputFiles,
        on_event: EventHandler,
    ) -> Result<Self> {
        let content = content::shareable_content()?;
        let Plan {
            filter,
            info,
            src_rect,
            cropped_window,
        } = plan(&content, options)?;
        let cfg = stream_cfg(
            info.width,
            info.height,
            info.fps,
            src_rect,
            options.system_audio,
        );

        let shared = Arc::new(Shared {
            video: Mutex::new(Some(MediaWriter::hevc(
                &files.video,
                info.width,
                info.height,
                info.fps,
            )?)),
            refit: Mutex::new(cropped_window.map(|(width, height)| Refit {
                watch: ResizeWatch::new(width, height),
                width: info.width,
                height: info.height,
                fps: info.fps,
                system_audio: options.system_audio,
            })),
            video_failed: Mutex::new(false),
            system_audio: Mutex::new(
                options
                    .system_audio
                    .then(|| AudioSlot::new(files.system_audio.clone())),
            ),
            on_event: on_event.clone(),
        });

        let sink = StreamSink::with(SinkInner {
            shared: shared.clone(),
        });
        let stream = sc::Stream::with_delegate(&filter, &cfg, sink.as_ref());
        let video_queue = dispatch::Queue::serial_with_ar_pool();
        let audio_queue = dispatch::Queue::serial_with_ar_pool();
        stream
            .add_stream_output(sink.as_ref(), sc::OutputType::Screen, Some(&video_queue))
            .map_err(|e| platform("cannot add video output", e))?;
        if options.system_audio {
            stream
                .add_stream_output(sink.as_ref(), sc::OutputType::Audio, Some(&audio_queue))
                .map_err(|e| platform("cannot add audio output", e))?;
        }

        let mic = if options.mic {
            Some(MicCapture::start(files.mic.clone(), on_event)?)
        } else {
            None
        };

        if let Err(e) = block_on(stream.start()) {
            if let Some(mic) = mic {
                let _ = mic.stop();
            }
            return Err(if cg::screen_capture_access::preflight() {
                platform("cannot start capture", &e)
            } else {
                Error::PermissionDenied
            });
        }

        Ok(Self {
            info,
            stream,
            sink,
            shared,
            mic,
            _queues: (video_queue, audio_queue),
        })
    }
}

impl ActiveCapture for Recorder {
    fn info(&self) -> &CaptureInfo {
        &self.info
    }

    fn stop(self: Box<Self>) -> Result<()> {
        let stop_time = cm::Clock::host_time_clock().time();
        let stopped = block_on(self.stream.stop());
        let _ = self
            .stream
            .remove_stream_output(self.sink.as_ref(), sc::OutputType::Screen);
        let mic = self.mic.map(MicCapture::stop).transpose();

        let video = self.shared.video.lock().expect("video lock").take();
        let audio = self.shared.system_audio.lock().expect("audio lock").take();
        let video = match video {
            Some(w) => w.finish_at(Some(stop_time)).map(|_| ()),
            None => Ok(()),
        };
        let audio = audio.map(AudioSlot::finish).transpose();

        if let Err(e) = stopped {
            return Err(platform("cannot stop capture", &e));
        }
        video?;
        audio?;
        mic?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ResizeWatch;

    #[test]
    fn a_resize_is_reported_once() {
        let mut watch = ResizeWatch::new(401.0, 301.0);
        assert!(!watch.resized(401.0, 301.0));
        assert!(!watch.resized(401.2, 300.9), "rounding is not a resize");
        assert!(watch.resized(801.0, 601.0));
        assert!(!watch.resized(801.0, 601.0));
        assert!(!watch.resized(401.0, 301.0));
    }
}
