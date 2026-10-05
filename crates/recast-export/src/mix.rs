use std::path::PathBuf;

use recast_project::SoundSettings;
use recast_zoom::Click;

use crate::{
    Result, SAMPLE_RATE,
    decode::AudioDecoder,
    sounds::{PackSounds, SoundKind},
};

fn ms_to_frames(ms: f64) -> i64 {
    (ms * SAMPLE_RATE as f64 / 1000.0).round() as i64
}

/// A recorded audio track to mix.
pub struct TrackInput {
    pub path: PathBuf,
    /// Start of the track on the recording's timeline.
    pub offset_ms: f64,
    pub gain: f32,
}

struct Track {
    decoder: AudioDecoder,
    /// Output frames of silence before the track starts.
    silence: u64,
    gain: f32,
}

/// Mixes recorded audio and click sounds into interleaved 48 kHz stereo, one
/// chunk at a time.
pub struct Mixer {
    tracks: Vec<Track>,
    clicks: Vec<(u64, SoundKind)>,
    first_click: usize,
    sounds: Option<PackSounds>,
    sound_gain: f32,
    position: u64,
    total: u64,
    scratch: Vec<f32>,
}

impl Mixer {
    /// `start_ms` is where the mix starts on the recording's timeline and
    /// `total_frames` its length in audio frames. Tracks that cannot be read are
    /// left out with a warning.
    pub fn new(
        inputs: Vec<TrackInput>,
        clicks: &[Click],
        sounds: &SoundSettings,
        start_ms: f64,
        total_frames: u64,
    ) -> Result<Self> {
        let mut tracks = Vec::new();
        for input in inputs {
            if input.gain <= 0.0 {
                continue;
            }
            let lead_ms = start_ms - input.offset_ms;
            let decoder = match AudioDecoder::open_at(&input.path, lead_ms.max(0.0)) {
                Ok(Some(decoder)) => decoder,
                Ok(None) => continue,
                Err(e) => {
                    log::warn!("skipping {}: {e}", input.path.display());
                    continue;
                }
            };
            tracks.push(Track {
                decoder,
                silence: (-ms_to_frames(lead_ms)).max(0) as u64,
                gain: input.gain,
            });
        }

        let end_ms = start_ms + total_frames as f64 * 1000.0 / SAMPLE_RATE as f64;
        let clicks: Vec<(u64, SoundKind)> = if sounds.enabled && sounds.volume > 0.0 {
            clicks
                .iter()
                .filter(|c| c.on_screen() && c.t_ms >= start_ms && c.t_ms < end_ms)
                .map(|c| {
                    (
                        ms_to_frames(c.t_ms - start_ms).max(0) as u64,
                        SoundKind::for_click(c.button, c.down, sounds.separate_left_right),
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        let sounds_pack = if clicks.is_empty() {
            None
        } else {
            Some(PackSounds::load(sounds.pack)?)
        };
        Ok(Self {
            tracks,
            clicks,
            first_click: 0,
            sounds: sounds_pack,
            sound_gain: sounds.volume as f32,
            position: 0,
            total: total_frames,
            scratch: Vec::new(),
        })
    }

    /// The next frame to be mixed.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Whether there is anything to hear.
    pub fn has_audio(&self) -> bool {
        !self.tracks.is_empty() || !self.clicks.is_empty()
    }

    /// The next chunk of at most `frames` frames: its first frame and its samples.
    pub fn next_chunk(&mut self, frames: usize) -> Result<Option<(u64, Vec<f32>)>> {
        if self.position >= self.total {
            return Ok(None);
        }
        let first = self.position;
        let frames = (frames as u64).min(self.total - first) as usize;
        let mut out = vec![0.0f32; frames * 2];

        for track in &mut self.tracks {
            let quiet = (track.silence.min(frames as u64)) as usize;
            track.silence -= quiet as u64;
            let audible = frames - quiet;
            if audible == 0 {
                continue;
            }
            self.scratch.resize(audible * 2, 0.0);
            track.decoder.read(&mut self.scratch)?;
            for (o, s) in out[quiet * 2..].iter_mut().zip(&self.scratch) {
                *o += s * track.gain;
            }
        }

        if let Some(sounds) = &self.sounds {
            let end = first + frames as u64;
            while let Some(&(start, kind)) = self.clicks.get(self.first_click) {
                if start + (sounds.get(kind).len() / 2) as u64 > first {
                    break;
                }
                self.first_click += 1;
            }
            for &(start, kind) in &self.clicks[self.first_click..] {
                if start >= end {
                    break;
                }
                let sound = sounds.get(kind);
                let sound_frames = (sound.len() / 2) as u64;
                let from = first.max(start);
                let to = end.min(start + sound_frames);
                for frame in from..to {
                    let o = ((frame - first) * 2) as usize;
                    let s = ((frame - start) * 2) as usize;
                    out[o] += sound[s] * self.sound_gain;
                    out[o + 1] += sound[s + 1] * self.sound_gain;
                }
            }
        }

        for s in &mut out {
            *s = s.clamp(-1.0, 1.0);
        }
        self.position += frames as u64;
        Ok(Some((first, out)))
    }
}

#[cfg(test)]
mod tests {
    use recast_project::{MouseButton, SoundPack};
    use recast_zoom::Point;

    use super::*;

    fn click(t_ms: f64, down: bool) -> Click {
        Click {
            t_ms,
            pos: Point::CENTER,
            button: MouseButton::Left,
            down,
        }
    }

    fn settings(volume: f64) -> SoundSettings {
        SoundSettings {
            enabled: true,
            pack: SoundPack::MouseClick,
            volume,
            separate_left_right: false,
        }
    }

    fn collect(mixer: &mut Mixer, chunk: usize) -> Vec<f32> {
        let mut all = Vec::new();
        let mut expected_first = 0;
        while let Some((first, samples)) = mixer.next_chunk(chunk).unwrap() {
            assert_eq!(first, expected_first);
            expected_first += (samples.len() / 2) as u64;
            all.extend(samples);
        }
        all
    }

    #[test]
    fn places_click_sounds_at_their_time() {
        let clicks = [
            click(500.0, true),
            click(1_100.0, false),
            click(5_000.0, true),
        ];
        let mut mixer = Mixer::new(Vec::new(), &clicks, &settings(1.0), 100.0, 96_000).unwrap();
        assert!(mixer.has_audio());
        let audio = collect(&mut mixer, 1_000);
        assert_eq!(audio.len(), 96_000 * 2);

        let sounds = PackSounds::load(SoundPack::MouseClick).unwrap();
        let down = sounds.get(SoundKind::LeftDown);
        let start = 400 * 48 * 2;
        assert!(audio[..start].iter().all(|s| *s == 0.0));
        for i in (0..down.len()).step_by(31) {
            assert!((audio[start + i] - down[i]).abs() < 1e-6);
        }
        let up_start = 1_000 * 48 * 2;
        assert!(audio[up_start..up_start + 2_000].iter().any(|s| *s != 0.0));
    }

    #[test]
    fn clicks_outside_the_recording_are_silent() {
        let mut outside = click(10.0, true);
        outside.pos = Point::new(1.4, 0.5);
        let mixer = Mixer::new(Vec::new(), &[outside], &settings(1.0), 0.0, 4_800).unwrap();
        assert!(!mixer.has_audio());
    }

    #[test]
    fn muted_or_disabled_sounds_are_silent() {
        let clicks = [click(10.0, true)];
        let mixer = Mixer::new(Vec::new(), &clicks, &settings(0.0), 0.0, 4_800).unwrap();
        assert!(!mixer.has_audio());
        let mut off = settings(1.0);
        off.enabled = false;
        let mixer = Mixer::new(Vec::new(), &clicks, &off, 0.0, 4_800).unwrap();
        assert!(!mixer.has_audio());
    }

    #[test]
    fn sound_overlapping_chunks_is_continuous() {
        let clicks = [click(0.0, true)];
        let mut small = Mixer::new(Vec::new(), &clicks, &settings(0.5), 0.0, 9_600).unwrap();
        let mut large = Mixer::new(Vec::new(), &clicks, &settings(0.5), 0.0, 9_600).unwrap();
        assert_eq!(collect(&mut small, 333), collect(&mut large, 9_600));
    }
}
