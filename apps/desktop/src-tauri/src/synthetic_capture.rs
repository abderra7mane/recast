//! A `ScreenCapture` that writes generated video and audio through the real writers,
//! for testing recording and crash recovery without Screen Recording permission.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use cidre::cm;
use recast_capture::{
    ActiveCapture, CaptureEvent, CaptureInfo, CaptureOptions, DisplayInfo, EventHandler,
    OutputFiles, Rect, Result, ScreenCapture, Track, WindowInfo,
    macos::{host_time_ns, synthetic, writer::MediaWriter},
};

pub struct SyntheticCapture {
    pub width: u32,
    pub height: u32,
}

impl ScreenCapture for SyntheticCapture {
    fn displays(&self) -> Result<Vec<DisplayInfo>> {
        Ok(vec![])
    }

    fn windows(&self) -> Result<Vec<WindowInfo>> {
        Ok(vec![])
    }

    fn start(
        &self,
        options: &CaptureOptions,
        files: &OutputFiles,
        on_event: EventHandler,
    ) -> Result<Box<dyn ActiveCapture>> {
        let (width, height, fps) = (self.width, self.height, options.fps);
        let mut video = MediaWriter::hevc(&files.video, width, height, fps)?;
        let mut audio = if options.system_audio {
            Some(MediaWriter::aac(&files.system_audio, 2)?)
        } else {
            None
        };
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = thread::spawn(move || {
            let frame = cm::Time::new(1, fps as i32);
            let start = cm::Clock::host_time_clock().time();
            let mut index: i64 = 0;
            let mut audio_frames: i64 = 0;
            while !flag.load(Ordering::Acquire) {
                let pts = start.add(cm::Time::new(index, fps as i32));
                if let Ok(buf) = synthetic::video_frame(width, height, index as u64, pts, frame) {
                    match video.append(&buf) {
                        Ok(Some(t)) => on_event(CaptureEvent::TrackStarted {
                            track: Track::Video,
                            host_ns: host_time_ns(t),
                        }),
                        Ok(None) => {}
                        Err(e) => on_event(CaptureEvent::Failed {
                            message: e.to_string(),
                        }),
                    }
                }
                if let Some(audio) = audio.as_mut() {
                    let due = (index + 1) * 48_000 / fps as i64;
                    while audio_frames + 1024 <= due {
                        let pts = start.add(cm::Time::new(audio_frames, 48_000));
                        if let Ok(buf) =
                            synthetic::audio_chunk(48_000.0, audio_frames as u64, 1024, pts)
                            && let Ok(Some(t)) = audio.append(&buf)
                        {
                            on_event(CaptureEvent::TrackStarted {
                                track: Track::SystemAudio,
                                host_ns: host_time_ns(t),
                            });
                        }
                        audio_frames += 1024;
                    }
                }
                index += 1;
                let next = start.add(cm::Time::new(index, fps as i32));
                let wait = next.sub(cm::Clock::host_time_clock().time()).as_secs();
                if wait > 0.0 {
                    thread::sleep(Duration::from_secs_f64(wait));
                }
            }
            let stop = cm::Clock::host_time_clock().time();
            let video = video.finish_at(Some(stop)).map(|_| ());
            let audio = audio.map(|a| a.finish().map(|_| ())).transpose();
            video.and(audio.map(|_| ()))
        });
        Ok(Box::new(SyntheticActive {
            info: CaptureInfo {
                width,
                height,
                scale_factor: 1.0,
                bounds: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: width as f64,
                    height: height as f64,
                },
                fps,
                codec: "hevc".into(),
                window_title: None,
                app_name: None,
            },
            stop,
            thread: Some(thread),
        }))
    }
}

struct SyntheticActive {
    info: CaptureInfo,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<()>>>,
}

impl ActiveCapture for SyntheticActive {
    fn info(&self) -> &CaptureInfo {
        &self.info
    }

    fn stop(mut self: Box<Self>) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        match self.thread.take().map(JoinHandle::join) {
            Some(Ok(result)) => result,
            Some(Err(_)) => Err(recast_capture::Error::Platform(
                "synthetic capture thread panicked".into(),
            )),
            None => Ok(()),
        }
    }
}
