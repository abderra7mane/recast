//! Minimal ISO-BMFF reader for (fragmented) MP4 files written by the recorder.
//! It finds where the last complete fragment ends and how long each track is.

use std::{
    collections::HashMap,
    fs,
    io::{Cursor, Read, Seek, SeekFrom},
    path::Path,
};

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Video,
    Audio,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackSummary {
    pub id: u32,
    pub kind: TrackKind,
    pub timescale: u32,
    pub duration: u64,
}

impl TrackSummary {
    pub fn duration_ms(&self) -> f64 {
        if self.timescale == 0 {
            return 0.0;
        }
        self.duration as f64 * 1000.0 / self.timescale as f64
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub tracks: Vec<TrackSummary>,
    pub fragments: usize,
    /// Bytes up to the end of the last complete top-level box (a fragment counts only with its media data).
    pub complete_len: u64,
    pub file_len: u64,
}

impl Summary {
    pub fn track(&self, kind: TrackKind) -> Option<&TrackSummary> {
        self.tracks.iter().find(|t| t.kind == kind)
    }

    pub fn duration_ms(&self) -> f64 {
        self.tracks
            .iter()
            .map(TrackSummary::duration_ms)
            .fold(0.0, f64::max)
    }
}

#[derive(Default)]
struct TrackState {
    kind: Option<TrackKind>,
    timescale: u32,
    mdhd_duration: u64,
    sample_table_duration: u64,
    default_sample_duration: u32,
    fragments_end: u64,
    fragments_sum: u64,
    has_tfdt: bool,
}

struct Fragment {
    track_id: u32,
    base_time: Option<u64>,
    duration: u64,
    samples_without_duration: u64,
}

/// Reads only box headers and the small `moov` / `moof` boxes, so large files are cheap to scan.
pub fn scan_file(path: &Path) -> Result<Summary> {
    let mut file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    scan_source(&mut file, len)
}

/// Cuts off a trailing partial box or orphan fragment header so other readers accept the file.
pub fn repair_file(path: &Path) -> Result<Summary> {
    let summary = scan_file(path)?;
    if summary.complete_len < summary.file_len {
        let file = fs::OpenOptions::new().write(true).open(path)?;
        file.set_len(summary.complete_len)?;
        file.sync_all()?;
    }
    Ok(Summary {
        file_len: summary.complete_len,
        ..summary
    })
}

pub fn scan(data: &[u8]) -> Result<Summary> {
    scan_source(&mut Cursor::new(data), data.len() as u64)
}

const MAX_HEADER_BOX: u64 = 256 * 1024 * 1024;

struct TopBox {
    kind: [u8; 4],
    body_start: u64,
    end: u64,
}

/// Next complete top-level box at `pos`, or `None` at the end or at a partial box.
fn next_top_box<R: Read + Seek>(r: &mut R, pos: u64, len: u64) -> Result<Option<TopBox>> {
    if pos + 8 > len {
        return Ok(None);
    }
    let mut header = [0u8; 16];
    r.seek(SeekFrom::Start(pos))?;
    r.read_exact(&mut header[..8])?;
    let size32 = u32::from_be_bytes(header[..4].try_into().expect("4"));
    let kind: [u8; 4] = header[4..8].try_into().expect("4");
    let (header_len, size) = match size32 {
        0 => (8, len - pos),
        1 => {
            if pos + 16 > len {
                return Ok(None);
            }
            r.read_exact(&mut header[8..16])?;
            (16, u64::from_be_bytes(header[8..16].try_into().expect("8")))
        }
        n => (8, n as u64),
    };
    if size < header_len || pos + size > len {
        return Ok(None);
    }
    Ok(Some(TopBox {
        kind,
        body_start: pos + header_len,
        end: pos + size,
    }))
}

fn read_body<R: Read + Seek>(r: &mut R, b: &TopBox) -> Result<Vec<u8>> {
    let size = b.end - b.body_start;
    if size > MAX_HEADER_BOX {
        return Err(Error::InvalidMedia("header box too large".into()));
    }
    let mut body = vec![0u8; size as usize];
    r.seek(SeekFrom::Start(b.body_start))?;
    r.read_exact(&mut body)?;
    Ok(body)
}

fn scan_source<R: Read + Seek>(r: &mut R, len: u64) -> Result<Summary> {
    let mut tracks: HashMap<u32, TrackState> = HashMap::new();
    let mut order: Vec<u32> = Vec::new();
    // Fragments come either as `moof` + `mdat` (ISO) or, as AVAssetWriter writes them,
    // `mdat` + `moof`. A fragment counts once both of its boxes are complete.
    let mut pending: Option<Vec<Fragment>> = None;
    let mut unclaimed_mdat = false;
    let mut fragments = 0usize;
    let mut complete_len = 0u64;
    let mut has_moov = false;

    let mut commit = |tracks: &mut HashMap<u32, TrackState>, parsed: Vec<Fragment>| {
        for fragment in parsed {
            apply_fragment(tracks, fragment);
        }
        fragments += 1;
    };

    let mut pos = 0u64;
    while let Some(top) = next_top_box(r, pos, len)? {
        pos = top.end;
        let end = top.end;
        match &top.kind {
            b"moov" => {
                has_moov = true;
                parse_moov(&read_body(r, &top)?, &mut tracks, &mut order)?;
                unclaimed_mdat = false;
                complete_len = end;
            }
            b"moof" => {
                let parsed = parse_moof(&read_body(r, &top)?)?;
                if unclaimed_mdat {
                    commit(&mut tracks, parsed);
                    unclaimed_mdat = false;
                    complete_len = end;
                } else {
                    pending = Some(parsed);
                }
            }
            b"mdat" => match pending.take() {
                Some(parsed) => {
                    commit(&mut tracks, parsed);
                    complete_len = end;
                }
                None => unclaimed_mdat = true,
            },
            _ => {
                if pending.is_none() && !unclaimed_mdat {
                    complete_len = end;
                }
            }
        }
    }

    if !has_moov {
        return Err(Error::InvalidMedia(
            "no movie header; it stopped before the first fragment was written".into(),
        ));
    }

    let tracks = order
        .iter()
        .filter_map(|id| tracks.get(id).map(|state| (*id, state)))
        .map(|(id, state)| {
            let fragments_end = if state.has_tfdt {
                state.fragments_end
            } else {
                state.sample_table_duration + state.fragments_sum
            };
            TrackSummary {
                id,
                kind: state.kind.unwrap_or(TrackKind::Other),
                timescale: state.timescale,
                duration: state
                    .sample_table_duration
                    .max(fragments_end)
                    .max(state.mdhd_duration),
            }
        })
        .collect();

    Ok(Summary {
        tracks,
        fragments,
        complete_len,
        file_len: len,
    })
}

fn apply_fragment(tracks: &mut HashMap<u32, TrackState>, fragment: Fragment) {
    let Some(state) = tracks.get_mut(&fragment.track_id) else {
        return;
    };
    let duration = fragment.duration
        + fragment.samples_without_duration * state.default_sample_duration as u64;
    state.fragments_sum += duration;
    if let Some(base) = fragment.base_time {
        state.has_tfdt = true;
        state.fragments_end = state.fragments_end.max(base + duration);
    }
}

/// Child boxes of a parent box body; stops at the first malformed child.
fn children(data: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> {
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        let header = data.get(pos..pos + 8)?;
        let size32 = u32::from_be_bytes(header[..4].try_into().ok()?);
        let kind: [u8; 4] = header[4..8].try_into().ok()?;
        let (header_len, size) = match size32 {
            0 => (8usize, data.len() - pos),
            1 => (
                16,
                u64::from_be_bytes(data.get(pos + 8..pos + 16)?.try_into().ok()?) as usize,
            ),
            n => (8, n as usize),
        };
        if size < header_len || pos + size > data.len() {
            return None;
        }
        let body = &data[pos + header_len..pos + size];
        pos += size;
        Some((kind, body))
    })
}

fn child<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    children(data).find(|(k, _)| k == kind).map(|(_, b)| b)
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let slice = self
            .data
            .get(self.pos..self.pos + n)
            .ok_or_else(|| Error::InvalidMedia("box too short".into()))?;
        self.pos += n;
        Ok(slice)
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().expect("4")))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().expect("8")))
    }

    fn version_flags(&mut self) -> Result<(u8, u32)> {
        let v = self.u32()?;
        Ok(((v >> 24) as u8, v & 0x00ff_ffff))
    }
}

fn parse_moov(
    moov: &[u8],
    tracks: &mut HashMap<u32, TrackState>,
    order: &mut Vec<u32>,
) -> Result<()> {
    for (kind, body) in children(moov) {
        match &kind {
            b"trak" => parse_trak(body, tracks, order)?,
            b"mvex" => {
                for (kind, trex) in children(body) {
                    if &kind == b"trex" {
                        let mut r = Reader::new(trex);
                        r.version_flags()?;
                        let track_id = r.u32()?;
                        r.u32()?;
                        let default_duration = r.u32()?;
                        tracks.entry(track_id).or_default().default_sample_duration =
                            default_duration;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn parse_trak(
    trak: &[u8],
    tracks: &mut HashMap<u32, TrackState>,
    order: &mut Vec<u32>,
) -> Result<()> {
    let tkhd = child(trak, b"tkhd").ok_or_else(|| Error::InvalidMedia("missing tkhd".into()))?;
    let mut r = Reader::new(tkhd);
    let (version, _) = r.version_flags()?;
    if version == 1 {
        r.take(16)?;
    } else {
        r.take(8)?;
    }
    let track_id = r.u32()?;
    if !order.contains(&track_id) {
        order.push(track_id);
    }
    let state = tracks.entry(track_id).or_default();

    let Some(mdia) = child(trak, b"mdia") else {
        return Ok(());
    };
    if let Some(mdhd) = child(mdia, b"mdhd") {
        let mut r = Reader::new(mdhd);
        let (version, _) = r.version_flags()?;
        if version == 1 {
            r.take(16)?;
            state.timescale = r.u32()?;
            state.mdhd_duration = r.u64()?;
        } else {
            r.take(8)?;
            state.timescale = r.u32()?;
            state.mdhd_duration = r.u32()? as u64;
        }
    }
    if let Some(hdlr) = child(mdia, b"hdlr") {
        let mut r = Reader::new(hdlr);
        r.version_flags()?;
        r.u32()?;
        state.kind = Some(match r.take(4)? {
            b"vide" => TrackKind::Video,
            b"soun" => TrackKind::Audio,
            _ => TrackKind::Other,
        });
    }
    let stts = child(mdia, b"minf")
        .and_then(|minf| child(minf, b"stbl"))
        .and_then(|stbl| child(stbl, b"stts"));
    if let Some(stts) = stts {
        let mut r = Reader::new(stts);
        r.version_flags()?;
        let entries = r.u32()?;
        let mut total = 0u64;
        for _ in 0..entries {
            let count = r.u32()? as u64;
            let delta = r.u32()? as u64;
            total += count * delta;
        }
        state.sample_table_duration = total;
    }
    Ok(())
}

fn parse_moof(moof: &[u8]) -> Result<Vec<Fragment>> {
    let mut fragments = Vec::new();
    for (kind, traf) in children(moof) {
        if &kind != b"traf" {
            continue;
        }
        let tfhd =
            child(traf, b"tfhd").ok_or_else(|| Error::InvalidMedia("missing tfhd".into()))?;
        let mut r = Reader::new(tfhd);
        let (_, flags) = r.version_flags()?;
        let track_id = r.u32()?;
        if flags & 0x01 != 0 {
            r.u64()?;
        }
        if flags & 0x02 != 0 {
            r.u32()?;
        }
        let default_duration = if flags & 0x08 != 0 {
            Some(r.u32()?)
        } else {
            None
        };

        let base_time = match child(traf, b"tfdt") {
            Some(tfdt) => {
                let mut r = Reader::new(tfdt);
                let (version, _) = r.version_flags()?;
                Some(if version == 1 {
                    r.u64()?
                } else {
                    r.u32()? as u64
                })
            }
            None => None,
        };

        let mut duration = 0u64;
        let mut samples_without_duration = 0u64;
        for (kind, trun) in children(traf) {
            if &kind == b"trun" {
                let run = trun_duration(trun)?;
                duration += run.explicit;
                match default_duration {
                    Some(d) => duration += run.samples_without_duration * d as u64,
                    None => samples_without_duration += run.samples_without_duration,
                }
            }
        }
        fragments.push(Fragment {
            track_id,
            base_time,
            duration,
            samples_without_duration,
        });
    }
    Ok(fragments)
}

struct RunDuration {
    explicit: u64,
    samples_without_duration: u64,
}

fn trun_duration(trun: &[u8]) -> Result<RunDuration> {
    let mut r = Reader::new(trun);
    let (_, flags) = r.version_flags()?;
    let count = r.u32()? as u64;
    if flags & 0x001 != 0 {
        r.u32()?;
    }
    if flags & 0x004 != 0 {
        r.u32()?;
    }
    if flags & 0x100 == 0 {
        return Ok(RunDuration {
            explicit: 0,
            samples_without_duration: count,
        });
    }
    let other_fields = [0x200u32, 0x400, 0x800]
        .iter()
        .filter(|f| flags & **f != 0)
        .count();
    let mut explicit = 0u64;
    for _ in 0..count {
        explicit += r.u32()? as u64;
        for _ in 0..other_fields {
            r.u32()?;
        }
    }
    Ok(RunDuration {
        explicit,
        samples_without_duration: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{AUDIO, VIDEO, fragment_parts, fragmented_file, header, mp4_box};

    /// AVAssetWriter's layout: media data first, then the fragment header describing it.
    fn data_first_file(fragments: u32, samples: u32, partial_tail: bool) -> (Vec<u8>, usize) {
        let mut out = header(&VIDEO);
        for i in 0..fragments {
            let base = (i * samples * VIDEO.default_duration) as u64;
            let (moof, mdat) = fragment_parts(&VIDEO, i + 1, base, samples);
            out.extend(mdat);
            out.extend(moof);
            out.extend(mp4_box(b"wide", b""));
        }
        let complete = out.len();
        if partial_tail {
            out.extend(0u32.to_be_bytes());
            out.extend(b"mdat");
            out.extend([0xCD; 100]);
        }
        (out, complete)
    }

    #[test]
    fn reads_data_first_fragments() {
        let (data, complete) = data_first_file(2, 120, true);
        let summary = scan(&data).unwrap();
        assert_eq!(summary.fragments, 2);
        assert_eq!(summary.complete_len, complete as u64);
        let video = summary.track(TrackKind::Video).unwrap();
        assert_eq!(video.duration_ms(), 4000.0);
    }

    #[test]
    fn data_first_without_header_is_dropped() {
        let (mut data, _) = data_first_file(1, 60, false);
        let complete = data.len();
        data.extend(mp4_box(b"mdat", &[1, 2, 3]));
        let summary = scan(&data).unwrap();
        assert_eq!(summary.fragments, 1);
        assert_eq!(summary.complete_len, complete as u64);
    }

    #[test]
    fn sums_fragment_durations() {
        let data = fragmented_file(&VIDEO, 3, 120);
        let summary = scan(&data).unwrap();
        assert_eq!(summary.fragments, 3);
        assert_eq!(summary.complete_len, data.len() as u64);
        let video = summary.track(TrackKind::Video).unwrap();
        assert_eq!(video.duration, 3 * 120 * 10);
        assert_eq!(video.duration_ms(), 6000.0);
    }

    #[test]
    fn ignores_truncated_fragment() {
        let full = fragmented_file(&AUDIO, 2, 100);
        let cut = full.len() - 50;
        let summary = scan(&full[..cut]).unwrap();
        assert_eq!(summary.fragments, 1);
        assert_eq!(summary.file_len, cut as u64);
        let one_fragment_len = fragmented_file(&AUDIO, 1, 100).len() as u64;
        assert_eq!(summary.complete_len, one_fragment_len);
        let audio = summary.track(TrackKind::Audio).unwrap();
        assert_eq!(audio.duration, 100 * 1024);
    }

    #[test]
    fn drops_moof_without_media_data() {
        let full = fragmented_file(&VIDEO, 2, 30);
        let first = fragmented_file(&VIDEO, 1, 30);
        let moof_end = full.len() - (30 * 4 + 8);
        let summary = scan(&full[..moof_end]).unwrap();
        assert_eq!(summary.fragments, 1);
        assert_eq!(summary.complete_len, first.len() as u64);
    }

    #[test]
    fn header_only_has_zero_duration() {
        let summary = scan(&header(&VIDEO)).unwrap();
        assert_eq!(summary.fragments, 0);
        assert_eq!(summary.duration_ms(), 0.0);
    }

    #[test]
    fn rejects_file_without_moov() {
        assert!(matches!(
            scan(&mp4_box(b"ftyp", b"isom")),
            Err(Error::InvalidMedia(_))
        ));
    }

    #[test]
    fn repair_truncates_partial_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.mp4");
        let full = fragmented_file(&VIDEO, 2, 30);
        std::fs::write(&path, &full[..full.len() - 7]).unwrap();
        let summary = repair_file(&path).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().len(),
            summary.complete_len
        );
        assert_eq!(scan_file(&path).unwrap().complete_len, summary.complete_len);
    }
}
