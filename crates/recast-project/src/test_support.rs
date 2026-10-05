//! Builds small fragmented MP4 files shaped like AVAssetWriter output.

pub fn mp4_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out
}

fn full_box(kind: &[u8; 4], version: u8, flags: u32, body: &[u8]) -> Vec<u8> {
    let mut content = (((version as u32) << 24) | flags).to_be_bytes().to_vec();
    content.extend_from_slice(body);
    mp4_box(kind, &content)
}

fn be(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}

pub struct TrackSpec {
    pub id: u32,
    pub handler: &'static [u8; 4],
    pub timescale: u32,
    pub default_duration: u32,
}

pub const VIDEO: TrackSpec = TrackSpec {
    id: 1,
    handler: b"vide",
    timescale: 600,
    default_duration: 10,
};

pub const AUDIO: TrackSpec = TrackSpec {
    id: 1,
    handler: b"soun",
    timescale: 48_000,
    default_duration: 1024,
};

pub fn header(track: &TrackSpec) -> Vec<u8> {
    let mut out = mp4_box(b"ftyp", b"mp42\0\0\0\x01mp41mp42isom");
    let tkhd = full_box(b"tkhd", 0, 3, &be(&[0, 0, track.id, 0, 0]));
    let mdhd = full_box(b"mdhd", 0, 0, &be(&[0, 0, track.timescale, 0, 0]));
    let mut hdlr_body = be(&[0]);
    hdlr_body.extend_from_slice(track.handler);
    hdlr_body.extend_from_slice(&[0u8; 13]);
    let hdlr = full_box(b"hdlr", 0, 0, &hdlr_body);
    let stts = full_box(b"stts", 0, 0, &be(&[0]));
    let stbl = mp4_box(b"stbl", &stts);
    let minf = mp4_box(b"minf", &stbl);
    let mdia = mp4_box(b"mdia", &[mdhd, hdlr, minf].concat());
    let trak = mp4_box(b"trak", &[tkhd, mdia].concat());
    let trex = full_box(
        b"trex",
        0,
        0,
        &be(&[track.id, 1, track.default_duration, 0, 0]),
    );
    let mvex = mp4_box(b"mvex", &trex);
    let mvhd = full_box(b"mvhd", 0, 0, &be(&[0, 0, track.timescale, 0]));
    out.extend(mp4_box(b"moov", &[mvhd, trak, mvex].concat()));
    out
}

/// One `moof` + `mdat` pair with `samples` samples starting at `base_time`.
pub fn fragment(track: &TrackSpec, sequence: u32, base_time: u64, samples: u32) -> Vec<u8> {
    let (moof, mdat) = fragment_parts(track, sequence, base_time, samples);
    [moof, mdat].concat()
}

/// The `moof` and `mdat` boxes of one fragment.
pub fn fragment_parts(
    track: &TrackSpec,
    sequence: u32,
    base_time: u64,
    samples: u32,
) -> (Vec<u8>, Vec<u8>) {
    let mfhd = full_box(b"mfhd", 0, 0, &be(&[sequence]));
    let tfhd = full_box(b"tfhd", 0, 0x020000, &be(&[track.id]));
    let tfdt = full_box(b"tfdt", 1, 0, &base_time.to_be_bytes());
    let mut trun_body = be(&[samples, 0]);
    for _ in 0..samples {
        trun_body.extend(be(&[track.default_duration, 4]));
    }
    let trun = full_box(b"trun", 0, 0x000301, &trun_body);
    let traf = mp4_box(b"traf", &[tfhd, tfdt, trun].concat());
    let moof = mp4_box(b"moof", &[mfhd, traf].concat());
    let mdat = mp4_box(b"mdat", &vec![0xAB; samples as usize * 4]);
    (moof, mdat)
}

/// A file with `fragments` fragments of `samples` samples each.
pub fn fragmented_file(track: &TrackSpec, fragments: u32, samples: u32) -> Vec<u8> {
    let mut out = header(track);
    for i in 0..fragments {
        let base = i as u64 * samples as u64 * track.default_duration as u64;
        out.extend(fragment(track, i + 1, base, samples));
    }
    out
}
