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
    ActiveCapture, CaptureEvent, CaptureInfo, CaptureOptions, CaptureTarget, Error, EventHandler,
    OutputFiles, Rect, Result, Track, check_region, even,
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

struct Shared {
    video: Mutex<Option<MediaWriter>>,
    video_failed: Mutex<bool>,
    system_audio: Mutex<Option<AudioSlot>>,
    on_event: EventHandler,
}

impl Shared {
    fn on_video(&self, buf: &cm::SampleBuf) {
        if buf.image_buf().is_none() || !frame_complete(buf) {
            return;
        }
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
        _stream: &sc::Stream,
        sample_buf: &mut cm::SampleBuf,
        kind: sc::OutputType,
    ) {
        let shared = &self.inner().shared;
        match kind {
            sc::OutputType::Screen => shared.on_video(sample_buf),
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
}

fn plan(content: &sc::ShareableContent, options: &CaptureOptions) -> Result<Plan> {
    let fps = options.fps.clamp(1, 120);
    let info = |width: f64, height: f64, scale: f64, bounds: Rect| CaptureInfo {
        width: even(width * scale),
        height: even(height * scale),
        scale_factor: scale,
        bounds,
        fps,
        codec: "hevc".into(),
        window_title: None,
        app_name: None,
    };
    match &options.target {
        CaptureTarget::Display { display_id } => {
            let display = content::find_display(content, *display_id)?;
            let filter = sc::ContentFilter::with_display_excluding_apps_excepting_windows(
                &display,
                &content::own_apps(content),
                &ns::Array::new(),
            );
            let scale = sc::ShareableContent::info_for_filter(&filter).point_pixel_scale() as f64;
            let bounds = content::rect(display.frame());
            Ok(Plan {
                info: info(bounds.width, bounds.height, scale, bounds),
                filter,
                src_rect: None,
            })
        }
        CaptureTarget::Region { display_id, rect } => {
            let display = content::find_display(content, *display_id)?;
            let frame = content::rect(display.frame());
            check_region(&frame, rect)?;
            let filter = sc::ContentFilter::with_display_excluding_apps_excepting_windows(
                &display,
                &content::own_apps(content),
                &ns::Array::new(),
            );
            let scale = sc::ShareableContent::info_for_filter(&filter).point_pixel_scale() as f64;
            let bounds = Rect {
                x: frame.x + rect.x,
                y: frame.y + rect.y,
                ..*rect
            };
            Ok(Plan {
                info: info(rect.width, rect.height, scale, bounds),
                filter,
                src_rect: Some(cg::Rect {
                    origin: cg::Point {
                        x: rect.x,
                        y: rect.y,
                    },
                    size: cg::Size {
                        width: rect.width,
                        height: rect.height,
                    },
                }),
            })
        }
        CaptureTarget::Window { window_id } => {
            let window = content::find_window(content, *window_id)?;
            let filter = sc::ContentFilter::with_desktop_independent_window(&window);
            let filter_info = sc::ShareableContent::info_for_filter(&filter);
            let scale = filter_info.point_pixel_scale() as f64;
            let size = filter_info.content_rect().size;
            let mut info = info(
                size.width,
                size.height,
                scale,
                content::rect(window.frame()),
            );
            info.window_title = window.title().map(|t| t.to_string());
            info.app_name = window.owning_app().map(|a| a.app_name().to_string());
            Ok(Plan {
                info,
                filter,
                src_rect: None,
            })
        }
    }
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
        } = plan(&content, options)?;

        let mut cfg = sc::StreamCfg::new();
        cfg.set_width(info.width as usize);
        cfg.set_height(info.height as usize);
        cfg.set_minimum_frame_interval(cm::Time::new(1, info.fps as i32));
        cfg.set_pixel_format(cv::PixelFormat::_420V);
        cfg.set_shows_cursor(false);
        cfg.set_queue_depth(6);
        cfg.set_capture_resolution(sc::CaptureResolution::Best);
        if let Some(rect) = src_rect {
            cfg.set_src_rect(rect);
        }
        cfg.set_captures_audio(options.system_audio);
        if options.system_audio {
            cfg.set_sample_rate(48_000);
            cfg.set_channel_count(2);
            cfg.set_excludes_current_process_audio(true);
        }

        let shared = Arc::new(Shared {
            video: Mutex::new(Some(MediaWriter::hevc(
                &files.video,
                info.width,
                info.height,
                info.fps,
            )?)),
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
