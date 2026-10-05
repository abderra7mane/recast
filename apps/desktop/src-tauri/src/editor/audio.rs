//! Preview audio: mixes the recorded tracks and click sounds from the playback
//! position and plays them on the default output device. What the device has
//! played is the playback clock; without a device a wall clock stands in.

use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use recast_export::{
    SAMPLE_RATE,
    mix::{Mixer, TrackInput},
};
use recast_project::SoundSettings;
use recast_zoom::Click;

/// Audio mixed ahead of the device.
const BUFFER_MS: f64 = 120.0;
const CHUNK_FRAMES: usize = 1_024;
/// The mix continues this long past the end of the recording, so the clock does too.
const TAIL_MS: f64 = 2_000.0;

#[derive(Debug, Clone, PartialEq)]
pub struct MixTrack {
    pub path: PathBuf,
    pub offset_ms: f64,
    pub gain: f32,
}

/// Everything that changes what is heard.
#[derive(Debug, Clone, PartialEq)]
pub struct MixSpec {
    pub tracks: Vec<MixTrack>,
    pub sounds: SoundSettings,
}

struct Control {
    generation: u64,
    playing: bool,
    start_ms: f64,
    mix: MixSpec,
    shutdown: bool,
}

struct Queue {
    generation: u64,
    playing: bool,
    samples: VecDeque<f32>,
    /// Timeline time of the first queued sample.
    next_ms: f64,
    start_ms: f64,
    /// When the sample at the given time is heard.
    anchor: Option<(Instant, f64)>,
    /// Set when there is no output device.
    wall_start: Option<Instant>,
}

struct Shared {
    control: Mutex<Control>,
    wake: Condvar,
    queue: Mutex<Queue>,
    /// Whether an output device is playing the mix; `None` until it was tried.
    output: Mutex<Option<bool>>,
}

pub struct AudioPlayer {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone, Copy)]
struct Output {
    rate: u32,
    channels: usize,
}

impl AudioPlayer {
    pub fn start(clicks: Vec<Click>, mix: MixSpec, duration_ms: f64) -> Self {
        Self::start_with(clicks, mix, duration_ms, true)
    }

    /// `use_device` false plays nothing and keeps time with the wall clock.
    fn start_with(clicks: Vec<Click>, mix: MixSpec, duration_ms: f64, use_device: bool) -> Self {
        let shared = Arc::new(Shared {
            control: Mutex::new(Control {
                generation: 0,
                playing: false,
                start_ms: 0.0,
                mix,
                shutdown: false,
            }),
            wake: Condvar::new(),
            queue: Mutex::new(Queue {
                generation: 0,
                playing: false,
                samples: VecDeque::new(),
                next_ms: 0.0,
                start_ms: 0.0,
                anchor: None,
                wall_start: None,
            }),
            output: Mutex::new(None),
        });
        let thread = {
            let shared = shared.clone();
            thread::Builder::new()
                .name("preview-audio".into())
                .spawn(move || run(&shared, &clicks, duration_ms, use_device))
                .ok()
        };
        Self { shared, thread }
    }

    /// Applies a playback request. The queue restarts at once, so the clock reads
    /// the new position before the audio thread has mixed anything for it.
    fn update(&self, change: impl FnOnce(&mut Control)) {
        let mut control = self.shared.control.lock().expect("audio control");
        change(&mut control);
        control.generation += 1;
        let no_device = *self.shared.output.lock().expect("audio output") == Some(false);
        let mut queue = self.shared.queue.lock().expect("audio queue");
        queue.generation = control.generation;
        queue.playing = false;
        queue.samples.clear();
        queue.start_ms = control.start_ms;
        queue.next_ms = control.start_ms;
        queue.anchor = None;
        queue.wall_start = (control.playing && no_device).then(Instant::now);
        drop(queue);
        self.shared.wake.notify_all();
    }

    pub fn play(&self, from_ms: f64) {
        self.update(|c| {
            c.playing = true;
            c.start_ms = from_ms;
        });
    }

    pub fn pause(&self) {
        self.update(|c| c.playing = false);
    }

    /// Applies a new mix; while playing it continues from the next unplayed sample.
    pub fn set_mix(&self, mix: MixSpec) {
        if self.shared.control.lock().expect("audio control").mix == mix {
            return;
        }
        let wall_clock = self
            .shared
            .queue
            .lock()
            .expect("audio queue")
            .wall_start
            .is_some();
        let resume_ms = if wall_clock {
            self.position_ms()
        } else {
            self.shared.queue.lock().expect("audio queue").next_ms
        };
        self.update(|c| {
            c.mix = mix;
            if c.playing {
                c.start_ms = resume_ms;
            }
        });
    }

    /// The timeline time being heard now.
    pub fn position_ms(&self) -> f64 {
        let queue = self.shared.queue.lock().expect("audio queue");
        if let Some(started) = queue.wall_start {
            return queue.start_ms + started.elapsed().as_secs_f64() * 1000.0;
        }
        match queue.anchor {
            Some((at, t_ms)) => {
                let now = Instant::now();
                let since = if now >= at {
                    now.duration_since(at).as_secs_f64() * 1000.0
                } else {
                    -(at.duration_since(now).as_secs_f64() * 1000.0)
                };
                (t_ms + since).clamp(queue.start_ms, queue.next_ms.max(queue.start_ms))
            }
            None => queue.start_ms,
        }
    }

    /// Whether audio plays on an output device; `None` while it is being opened.
    pub fn has_output(&self) -> Option<bool> {
        *self.shared.output.lock().expect("audio output")
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.update(|c| c.shutdown = true);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn open_output(shared: &Arc<Shared>) -> Option<(cpal::Stream, Output)> {
    let device = cpal::default_host().default_output_device()?;
    let config = match device.default_output_config() {
        Ok(config) => config,
        Err(e) => {
            log::warn!("no audio output config: {e}");
            return None;
        }
    };
    let output = Output {
        rate: config.sample_rate(),
        channels: config.channels().max(1) as usize,
    };
    let stream_config = cpal::StreamConfig {
        channels: config.channels(),
        sample_rate: config.sample_rate(),
        buffer_size: cpal::BufferSize::Default,
    };
    let callback_shared = shared.clone();
    let stream = device.build_output_stream::<f32, _, _>(
        stream_config,
        move |data, info| fill(&callback_shared, output, data, info),
        |e| log::warn!("audio output: {e}"),
        None,
    );
    let stream = match stream {
        Ok(stream) => stream,
        Err(e) => {
            log::warn!("cannot open audio output: {e}");
            return None;
        }
    };
    if let Err(e) = stream.play() {
        log::warn!("cannot start audio output: {e}");
        return None;
    }
    Some((stream, output))
}

/// The device callback: hands queued samples to the device and moves the clock.
fn fill(shared: &Shared, output: Output, data: &mut [f32], info: &cpal::OutputCallbackInfo) {
    let Ok(mut queue) = shared.queue.try_lock() else {
        data.fill(0.0);
        return;
    };
    if !queue.playing {
        data.fill(0.0);
        return;
    }
    let frames = data.len() / output.channels;
    let available = queue.samples.len() / output.channels;
    let n = frames.min(available) * output.channels;
    for (out, sample) in data.iter_mut().zip(queue.samples.drain(..n)) {
        *out = sample;
    }
    data[n..].fill(0.0);
    if n > 0 {
        let stamp = info.timestamp();
        let latency = stamp.playback.duration_since(stamp.callback);
        let first_ms = queue.next_ms;
        queue.anchor = Some((Instant::now() + latency, first_ms));
        queue.next_ms += (n / output.channels) as f64 * 1000.0 / output.rate as f64;
    }
}

fn run(shared: &Arc<Shared>, clicks: &[Click], duration_ms: f64, use_device: bool) {
    let device = if use_device {
        open_output(shared)
    } else {
        None
    };
    let output = device.as_ref().map(|(_, o)| *o);
    *shared.output.lock().expect("audio output") = Some(output.is_some());
    let mut seen = u64::MAX;
    let mut mixer: Option<Mixer> = None;
    let mut resampler = Resampler::default();
    let mut control = shared.control.lock().expect("audio control");
    loop {
        if control.shutdown {
            return;
        }
        if control.generation != seen {
            seen = control.generation;
            let (playing, start_ms, mix) = (control.playing, control.start_ms, control.mix.clone());
            drop(control);
            mixer = if playing && output.is_some() {
                build_mixer(&mix, clicks, start_ms, duration_ms)
            } else {
                None
            };
            resampler = Resampler::default();
            let mut queue = shared.queue.lock().expect("audio queue");
            if queue.generation == seen {
                queue.playing = playing;
                if playing && output.is_none() && queue.wall_start.is_none() {
                    queue.wall_start = Some(Instant::now());
                }
            }
            drop(queue);
            control = shared.control.lock().expect("audio control");
            continue;
        }
        if let (Some(mix), Some(output)) = (mixer.as_mut(), output) {
            let target = (BUFFER_MS * output.rate as f64 / 1000.0) as usize * output.channels;
            drop(control);
            while shared.queue.lock().expect("audio queue").samples.len() < target {
                let chunk = match mix.next_chunk(CHUNK_FRAMES) {
                    Ok(Some((_, samples))) => samples,
                    Ok(None) => vec![0.0; CHUNK_FRAMES * 2],
                    Err(e) => {
                        log::warn!("preview audio: {e}");
                        vec![0.0; CHUNK_FRAMES * 2]
                    }
                };
                let mut converted = Vec::with_capacity(chunk.len() * output.channels);
                resampler.process(&chunk, output, &mut converted);
                let mut queue = shared.queue.lock().expect("audio queue");
                if queue.generation != seen {
                    break;
                }
                queue.samples.extend(converted);
            }
            control = shared.control.lock().expect("audio control");
            control = shared
                .wake
                .wait_timeout(control, Duration::from_millis(10))
                .expect("audio control")
                .0;
        } else {
            control = shared.wake.wait(control).expect("audio control");
        }
    }
}

fn build_mixer(mix: &MixSpec, clicks: &[Click], start_ms: f64, duration_ms: f64) -> Option<Mixer> {
    let tracks = mix
        .tracks
        .iter()
        .map(|t| TrackInput {
            path: t.path.clone(),
            offset_ms: t.offset_ms,
            gain: t.gain,
        })
        .collect();
    let frames = ((duration_ms + TAIL_MS - start_ms).max(0.0) * SAMPLE_RATE as f64 / 1000.0) as u64;
    match Mixer::new(tracks, clicks, &mix.sounds, start_ms, frames) {
        Ok(mixer) => Some(mixer),
        Err(e) => {
            log::warn!("preview audio: {e}");
            None
        }
    }
}

/// Converts 48 kHz stereo to the device's rate and channel count, interpolating
/// linearly across chunk boundaries.
#[derive(Debug, Default)]
struct Resampler {
    /// Position of the next output frame; -1 is the last frame of the previous chunk.
    pos: f64,
    previous: [f32; 2],
}

impl Resampler {
    fn process(&mut self, input: &[f32], output: Output, out: &mut Vec<f32>) {
        let push = |out: &mut Vec<f32>, [l, r]: [f32; 2]| match output.channels {
            1 => out.push((l + r) * 0.5),
            n => {
                out.push(l);
                out.push(r);
                out.extend(std::iter::repeat_n(0.0, n - 2));
            }
        };
        let frames = input.len() / 2;
        if output.rate == SAMPLE_RATE {
            for frame in input.chunks_exact(2) {
                push(out, [frame[0], frame[1]]);
            }
            return;
        }
        if frames == 0 {
            return;
        }
        let step = SAMPLE_RATE as f64 / output.rate as f64;
        let frame = |i: isize| -> [f32; 2] {
            if i < 0 {
                self.previous
            } else {
                let i = i as usize * 2;
                [input[i], input[i + 1]]
            }
        };
        let mut pos = self.pos;
        while pos < (frames - 1) as f64 {
            let i = pos.floor();
            let f = (pos - i) as f32;
            let (a, b) = (frame(i as isize), frame(i as isize + 1));
            push(out, [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]);
            pos += step;
        }
        self.pos = pos - frames as f64;
        self.previous = frame(frames as isize - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(from: usize, frames: usize) -> Vec<f32> {
        (from..from + frames)
            .flat_map(|i| [i as f32, -(i as f32)])
            .collect()
    }

    fn silent_mix() -> MixSpec {
        MixSpec {
            tracks: Vec::new(),
            sounds: SoundSettings {
                enabled: false,
                ..Default::default()
            },
        }
    }

    fn silent_player() -> AudioPlayer {
        silent_player_with(true)
    }

    fn silent_player_with(use_device: bool) -> AudioPlayer {
        let player = AudioPlayer::start_with(Vec::new(), silent_mix(), 10_000.0, use_device);
        while player.has_output().is_none() {
            std::thread::sleep(Duration::from_millis(5));
        }
        player
    }

    #[test]
    fn the_clock_restarts_at_once_on_play() {
        let player = silent_player();
        player.play(4_000.0);
        assert_eq!(player.position_ms(), 4_000.0);
        std::thread::sleep(Duration::from_millis(400));
        let played = player.position_ms();
        assert!(played > 4_150.0 && played < 4_500.0, "{played}");

        player.play(1_000.0);
        let restarted = player.position_ms();
        assert!((1_000.0..1_050.0).contains(&restarted), "{restarted}");
        std::thread::sleep(Duration::from_millis(400));
        assert!(player.position_ms() > 1_150.0);

        player.pause();
        player.play(500.0);
        assert!(player.position_ms() < 550.0);
    }

    #[test]
    fn a_mix_change_keeps_the_position() {
        for use_device in [true, false] {
            let player = silent_player_with(use_device);
            player.play(1_000.0);
            std::thread::sleep(Duration::from_millis(400));
            let before = player.position_ms();
            assert!(before > 1_150.0, "{before}");
            player.set_mix(MixSpec {
                sounds: SoundSettings {
                    volume: 0.2,
                    ..silent_mix().sounds
                },
                ..silent_mix()
            });
            let after = player.position_ms();
            assert!(
                after >= before - 150.0,
                "device {use_device}: {before} then {after}"
            );
        }
    }

    #[test]
    fn same_rate_maps_channels() {
        let mut resampler = Resampler::default();
        let mut out = Vec::new();
        let output = Output {
            rate: SAMPLE_RATE,
            channels: 4,
        };
        resampler.process(&[0.5, -0.5, 1.0, 0.0], output, &mut out);
        assert_eq!(out, [0.5, -0.5, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
        let mono = Output {
            rate: SAMPLE_RATE,
            channels: 1,
        };
        out.clear();
        resampler.process(&[0.5, -0.5, 1.0, 0.0], mono, &mut out);
        assert_eq!(out, [0.0, 0.5]);
    }

    #[test]
    fn resampling_is_continuous_across_chunks() {
        let output = Output {
            rate: 44_100,
            channels: 2,
        };
        let mut whole = Vec::new();
        Resampler::default().process(&ramp(0, 4_800), output, &mut whole);
        let mut chunked = Vec::new();
        let mut resampler = Resampler::default();
        for start in (0..4_800).step_by(333) {
            let frames = 333.min(4_800 - start);
            resampler.process(&ramp(start, frames), output, &mut chunked);
        }
        let n = whole.len().min(chunked.len());
        assert!(n > 4_300 * 2);
        for (a, b) in whole[..n].iter().zip(&chunked[..n]) {
            assert!((a - b).abs() < 1e-3, "{a} vs {b}");
        }
        // A ramp stays a ramp: each output frame advances by 48000 / 44100 input frames.
        let step = 48_000.0 / 44_100.0;
        for (k, frame) in chunked.chunks_exact(2).take(1_000).enumerate() {
            assert!((frame[0] as f64 - k as f64 * step).abs() < 1e-2);
        }
    }
}
