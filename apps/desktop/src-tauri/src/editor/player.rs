//! The playback thread: owns the decoder, the compositor and the audio, renders
//! frames at the preview size and streams them as NV12. Commands are queued and
//! coalesced, so only the newest seek and the newest settings are acted on.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use recast_export::decode::VideoDecoder;
use recast_project::{BackgroundFill, EditSettings, EventLog, Project, Resolution};
use recast_render::{
    Compositor, CpuFrame, FrameSource, PixelFormat, Scene, bitmap::Rgba, nv12, resolve,
};

use super::{
    audio::{AudioPlayer, MixSpec, MixTrack},
    protocol::{FrameHeader, HEADER_LEN},
    server::PreviewServer,
};

/// Longest side of a preview frame in pixels.
pub const MAX_PREVIEW_SIDE: u32 = 1280;
const FRAME_INTERVAL: Duration = Duration::from_micros(16_667);
/// Forward jumps up to this long decode on from the current position instead of seeking.
const FORWARD_DECODE_MS: f64 = 1_000.0;
const IDLE_WAIT: Duration = Duration::from_secs(1);

pub enum Command {
    Play,
    Pause,
    Seek(f64),
    Loop(bool),
    Resize(u32, u32),
    Settings(Box<EditSettings>),
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PlaybackStatus {
    pub playing: bool,
    pub position_ms: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PlayerStats {
    pub frames: u64,
    /// Average time to decode, render, read back and convert one frame.
    pub render_ms: f64,
    /// From receiving a seek while paused to queueing its frame.
    pub last_seek_ms: f64,
    pub width: u32,
    pub height: u32,
    pub audio_output: Option<bool>,
}

#[derive(Default)]
struct Published {
    status: PlaybackStatus,
    stats: PlayerStats,
    error: Option<String>,
}

pub struct Player {
    commands: Option<mpsc::Sender<Command>>,
    published: Arc<Mutex<Published>>,
    thread: Option<JoinHandle<()>>,
}

pub struct PlayerInit {
    pub bundle_dir: PathBuf,
    pub project: Project,
    pub events: Arc<EventLog>,
    pub settings: EditSettings,
    pub server: Arc<PreviewServer>,
}

impl Player {
    pub fn start(init: PlayerInit) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let published = Arc::new(Mutex::new(Published::default()));
        let thread = {
            let published = published.clone();
            thread::Builder::new()
                .name("preview-player".into())
                .spawn(move || match State::new(init) {
                    Ok(mut state) => {
                        let _ = ready_tx.send(Ok(()));
                        state.run(&rx, &published);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                })
                .map_err(|e| e.to_string())?
        };
        ready_rx
            .recv()
            .map_err(|_| "the preview stopped while starting".to_string())??;
        Ok(Self {
            commands: Some(tx),
            published,
            thread: Some(thread),
        })
    }

    pub fn send(&self, command: Command) {
        if let Some(tx) = &self.commands {
            let _ = tx.send(command);
        }
    }

    pub fn status(&self) -> PlaybackStatus {
        self.published.lock().expect("player status").status
    }

    pub fn stats(&self) -> PlayerStats {
        self.published.lock().expect("player status").stats
    }

    /// The last rendering error, if the preview cannot show frames.
    pub fn error(&self) -> Option<String> {
        self.published.lock().expect("player status").error.clone()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.commands.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The recording's video, decoded on demand. Backward jumps and long forward jumps
/// reopen the reader at the new time; short forward steps keep decoding.
struct VideoSource {
    path: PathBuf,
    duration_ms: f64,
    decoder: Option<VideoDecoder>,
    last_ms: f64,
}

impl VideoSource {
    fn open_at(&mut self, t_ms: f64) -> recast_render::Result<()> {
        self.decoder = Some(
            VideoDecoder::open(&self.path, t_ms)
                .map_err(|e| recast_render::Error::Source(e.to_string()))?,
        );
        Ok(())
    }
}

impl FrameSource for VideoSource {
    fn frame_at(&mut self, t_ms: f64) -> recast_render::Result<CpuFrame<'_>> {
        let t_ms = t_ms.clamp(0.0, (self.duration_ms - 1.0).max(0.0));
        let reusable = self.decoder.is_some()
            && t_ms + 0.5 >= self.last_ms
            && t_ms - self.last_ms <= FORWARD_DECODE_MS;
        if !reusable {
            self.open_at(t_ms)?;
        }
        self.last_ms = t_ms;
        let ok = self
            .decoder
            .as_mut()
            .is_some_and(|d| d.frame_at(t_ms).is_ok());
        if !ok {
            // Past the last frame the reader returns nothing; start a little earlier.
            self.open_at((t_ms - 2_000.0).max(0.0))?;
        }
        self.decoder
            .as_mut()
            .expect("decoder opened")
            .frame_at(t_ms)
    }
}

pub fn mix_spec(
    bundle_dir: &std::path::Path,
    project: &Project,
    settings: &EditSettings,
) -> MixSpec {
    let recording = &project.recording;
    let tracks = [
        (&recording.system_audio, settings.audio.system_volume),
        (&recording.mic, settings.audio.mic_volume),
    ]
    .into_iter()
    .filter_map(|(track, gain)| {
        let track = track.as_ref()?;
        (gain > 0.0).then(|| MixTrack {
            path: bundle_dir.join(&track.file),
            offset_ms: track.offset_ms,
            gain: gain as f32,
        })
    })
    .collect();
    MixSpec {
        tracks,
        sounds: settings.sounds.clone(),
    }
}

/// Preview frame size: the output's aspect ratio fitted into `area` (device
/// pixels), no longer than [`MAX_PREVIEW_SIDE`], with even sides.
pub fn preview_size(output: (u32, u32), area: (u32, u32)) -> (u32, u32) {
    let (ow, oh) = (output.0.max(1) as f64, output.1.max(1) as f64);
    let (aw, ah) = if area.0 < 2 || area.1 < 2 {
        (MAX_PREVIEW_SIDE as f64, MAX_PREVIEW_SIDE as f64)
    } else {
        (area.0 as f64, area.1 as f64)
    };
    let scale = (aw / ow)
        .min(ah / oh)
        .min(MAX_PREVIEW_SIDE as f64 / ow.max(oh));
    let even = |v: f64| {
        let v = v.round().max(2.0) as u32;
        v - v % 2
    };
    (even(ow * scale), even(oh * scale))
}

fn image_path(settings: &EditSettings) -> Option<&str> {
    match &settings.background.fill {
        BackgroundFill::Image { path } => Some(path),
        _ => None,
    }
}

struct State {
    bundle_dir: PathBuf,
    project: Project,
    events: Arc<EventLog>,
    server: Arc<PreviewServer>,
    scene: Scene,
    compositor: Compositor,
    video: VideoSource,
    audio: AudioPlayer,
    area: (u32, u32),
    playing: bool,
    looping: bool,
    position_ms: f64,
    seq: u64,
    stats: PlayerStats,
    render_total_ms: f64,
}

impl State {
    fn new(init: PlayerInit) -> Result<Self, String> {
        let PlayerInit {
            bundle_dir,
            project,
            events,
            settings,
            server,
        } = init;
        let mut base = settings.clone();
        base.background.fill = BackgroundFill::Solid {
            color: recast_project::Color::rgb(0, 0, 0),
        };
        let scene =
            Scene::from_project(&bundle_dir, &project, &events, base).map_err(|e| e.to_string())?;
        let compositor = Compositor::new(2, 2, PixelFormat::Rgba8).map_err(|e| e.to_string())?;
        let duration_ms = project.recording.duration_ms;
        let audio = AudioPlayer::start(
            scene.timeline.clicks().to_vec(),
            mix_spec(&bundle_dir, &project, &settings),
            duration_ms,
        );
        let mut state = Self {
            video: VideoSource {
                path: bundle_dir.join(&project.recording.video.file),
                duration_ms,
                decoder: None,
                last_ms: 0.0,
            },
            bundle_dir,
            project,
            events,
            server,
            compositor,
            audio,
            area: (0, 0),
            playing: false,
            looping: false,
            position_ms: 0.0,
            seq: 0,
            stats: PlayerStats::default(),
            render_total_ms: 0.0,
            scene,
        };
        state.apply_settings(settings);
        state.position_ms = state.trim().0;
        Ok(state)
    }

    fn duration_ms(&self) -> f64 {
        self.project.recording.duration_ms
    }

    fn trim(&self) -> (f64, f64) {
        self.scene.settings.trim_range(self.duration_ms())
    }

    fn apply_settings(&mut self, settings: EditSettings) {
        let old_image = image_path(&self.scene.settings).map(str::to_owned);
        self.scene.set_settings(&self.events, settings);
        let new_image = image_path(&self.scene.settings).map(str::to_owned);
        if new_image != old_image {
            let image = new_image.and_then(|path| {
                Rgba::load(&resolve(&self.bundle_dir, &path))
                    .map_err(|e| log::warn!("background image: {e}"))
                    .ok()
            });
            self.scene.set_background(image);
        }
        self.audio.set_mix(mix_spec(
            &self.bundle_dir,
            &self.project,
            &self.scene.settings,
        ));
    }

    fn run(&mut self, commands: &mpsc::Receiver<Command>, published: &Mutex<Published>) {
        let mut next_tick = Instant::now();
        let mut render = true;
        let mut seek_started: Option<Instant> = None;
        loop {
            let timeout = if self.playing {
                next_tick.saturating_duration_since(Instant::now())
            } else if render {
                Duration::ZERO
            } else {
                IDLE_WAIT
            };
            let mut pending = Vec::new();
            match commands.recv_timeout(timeout) {
                Ok(command) => pending.push(command),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
            pending.extend(commands.try_iter());

            let mut seek = None;
            let mut settings = None;
            let mut play = None;
            for command in pending {
                match command {
                    Command::Play => play = Some(true),
                    Command::Pause => play = Some(false),
                    Command::Seek(t) => seek = Some(t),
                    Command::Loop(on) => self.looping = on,
                    Command::Resize(w, h) => {
                        self.area = (w, h);
                        render = true;
                    }
                    Command::Settings(s) => settings = Some(*s),
                }
            }
            if let Some(settings) = settings {
                self.apply_settings(settings);
                render = true;
            }
            if let Some(t) = seek {
                self.position_ms = t.clamp(0.0, self.duration_ms());
                if self.playing {
                    self.audio.play(self.position_ms);
                } else {
                    seek_started = Some(Instant::now());
                }
                render = true;
            }
            match play {
                Some(true) if !self.playing => {
                    let (start, end) = self.trim();
                    if self.position_ms < start || self.position_ms >= end - 1.0 {
                        self.position_ms = start;
                    }
                    self.playing = true;
                    self.audio.play(self.position_ms);
                    next_tick = Instant::now();
                }
                Some(false) if self.playing => {
                    self.position_ms = self.audio.position_ms();
                    self.playing = false;
                    self.audio.pause();
                    render = true;
                }
                _ => {}
            }

            if self.playing {
                let now = Instant::now();
                if now < next_tick {
                    continue;
                }
                next_tick = (next_tick + FRAME_INTERVAL).max(now);
                let (start, end) = self.trim();
                let t = self.audio.position_ms();
                if t >= end {
                    if self.looping && end > start {
                        self.position_ms = start;
                        self.audio.play(start);
                    } else {
                        self.position_ms = end;
                        self.playing = false;
                        self.audio.pause();
                    }
                } else {
                    self.position_ms = t;
                }
                render = true;
            }

            if render {
                render = false;
                let started = Instant::now();
                match self.render_frame() {
                    Ok(()) => {
                        let ms = started.elapsed().as_secs_f64() * 1000.0;
                        self.stats.frames += 1;
                        self.render_total_ms += ms;
                        self.stats.render_ms = self.render_total_ms / self.stats.frames as f64;
                        if let Some(seek) = seek_started.take() {
                            self.stats.last_seek_ms = seek.elapsed().as_secs_f64() * 1000.0;
                        }
                        published.lock().expect("player status").error = None;
                    }
                    Err(e) => {
                        log::warn!("preview frame: {e}");
                        published.lock().expect("player status").error = Some(e);
                    }
                }
            }
            let mut out = published.lock().expect("player status");
            out.status = PlaybackStatus {
                playing: self.playing,
                position_ms: self.position_ms,
            };
            out.stats = PlayerStats {
                audio_output: self.audio.has_output(),
                ..self.stats
            };
        }
    }

    fn render_frame(&mut self) -> Result<(), String> {
        let output = self.scene.output_size_for(Resolution::P1080);
        let (width, height) = preview_size(output, self.area);
        self.compositor
            .resize(width, height)
            .map_err(|e| e.to_string())?;
        let t_ms = self.position_ms;
        self.compositor
            .render_from(&self.scene, &mut self.video, t_ms)
            .map_err(|e| e.to_string())?;
        self.seq += 1;
        let header = FrameHeader {
            width,
            height,
            playing: self.playing,
            seq: self.seq,
            t_ms,
        };
        let mut message = vec![0u8; header.message_len()];
        message[..HEADER_LEN].copy_from_slice(&header.encode());
        self.compositor
            .read(|data, stride| {
                nv12::convert(
                    data,
                    stride,
                    width,
                    height,
                    PixelFormat::Rgba8,
                    &mut message[HEADER_LEN..],
                )
            })
            .map_err(|e| e.to_string())?;
        self.server.send(message);
        self.stats.width = width;
        self.stats.height = height;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_fits_the_area_and_caps_the_size() {
        assert_eq!(preview_size((1920, 1080), (960, 1000)), (960, 540));
        assert_eq!(preview_size((1920, 1080), (4000, 4000)), (1280, 720));
        assert_eq!(preview_size((1080, 1920), (4000, 4000)), (720, 1280));
        assert_eq!(preview_size((1920, 1080), (0, 0)), (1280, 720));
        let (w, h) = preview_size((1662, 1080), (1001, 999));
        assert!(w % 2 == 0 && h % 2 == 0 && w <= 1001 && h <= 999);
    }
}
