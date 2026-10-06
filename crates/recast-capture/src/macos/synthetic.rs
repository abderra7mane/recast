//! Generated media for exercising the writers without screen or microphone access.

use cidre::{arc, cat, cm, cv};

use crate::{Error, Result};

fn os_err(context: &str, e: impl std::fmt::Debug) -> Error {
    Error::Platform(format!("{context}: {e:?}"))
}

/// A 4:2:0 frame with a moving bar, stamped at `pts`.
pub fn video_frame(
    width: u32,
    height: u32,
    index: u64,
    pts: cm::Time,
    duration: cm::Time,
) -> Result<arc::R<cm::SampleBuf>> {
    let mut pixels = cv::PixelBuf::new(
        width as usize,
        height as usize,
        cv::PixelFormat::_420V,
        None,
    )
    .map_err(|e| os_err("pixel buffer", e))?;
    // SAFETY: the buffer is unlocked again below and not shared yet.
    unsafe { pixels.lock_base_addr(Default::default()) }
        .result()
        .map_err(|e| os_err("lock", e))?;
    {
        let bar = (index as usize * 8) % width as usize;
        for plane in 0..pixels.plane_count() {
            let rows = pixels.plane_height(plane);
            let stride = pixels.plane_bytes_per_row(plane);
            let base = pixels.plane_base_address(plane) as *mut u8;
            for y in 0..rows {
                // SAFETY: the plane is locked and `stride * rows` bytes long.
                let row = unsafe { std::slice::from_raw_parts_mut(base.add(y * stride), stride) };
                for (x, px) in row.iter_mut().enumerate() {
                    *px = if plane == 0 {
                        if (x as isize - bar as isize).abs() < 12 {
                            235
                        } else {
                            (16 + (y * 200 / rows.max(1))) as u8
                        }
                    } else {
                        128
                    };
                }
            }
        }
    }
    // SAFETY: matches the lock above.
    let _ = unsafe { pixels.unlock_lock_base_addr(Default::default()) };
    let desc = cm::VideoFormatDesc::with_image_buf(&pixels).map_err(|e| os_err("format", e))?;
    let timing = cm::SampleTimingInfo {
        duration,
        pts,
        dts: cm::Time::invalid(),
    };
    cm::SampleBuf::with_image_buf(&pixels, true, None, std::ptr::null(), &desc, &timing)
        .map_err(|e| os_err("sample buffer", e))
}

/// Screen-like luma with 1-pixel detail: a checkerboard, thin lines and rows of small
/// antialiased glyphs on white. Each step of `typed` adds glyphs at the end of the text.
pub fn text_luma(width: usize, height: usize, typed: usize) -> Vec<u8> {
    const PAPER: u8 = 235;
    const INK: u8 = 30;
    let mut luma = vec![PAPER; width * height];
    for (y, row) in luma.chunks_exact_mut(width).take(60).enumerate() {
        for (x, px) in row.iter_mut().enumerate() {
            *px = match y {
                0..20 if (x + y) % 2 == 0 => INK,
                0..20 => PAPER,
                20..40 if x % 3 == 0 => INK,
                40..60 if y % 3 == 0 => INK,
                _ => PAPER,
            };
        }
    }
    let (cell_w, line_h) = (7, 16);
    let per_line = (width.saturating_sub(16)) / cell_w;
    let lines = (height.saturating_sub(70)) / line_h;
    let shown = (per_line * lines * 3 / 4 + typed * 3).min(per_line * lines);
    for n in 0..shown {
        if n % 9 == 8 {
            continue;
        }
        let (left, top) = (8 + (n % per_line) * cell_w, 70 + (n / per_line) * line_h);
        let glyph = glyph(n as u64);
        for (gy, row) in glyph.iter().enumerate() {
            for (gx, coverage) in row.iter().enumerate() {
                luma[(top + gy) * width + left + gx] =
                    PAPER - (*coverage as u32 * (PAPER - INK) as u32 / 4) as u8;
            }
        }
    }
    luma
}

/// A 7×11 glyph of a few random strokes, antialiased from a 2× mask: each value is
/// the covered quarter count, 0..=4.
fn glyph(seed: u64) -> [[u8; 7]; 11] {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut mask = [[false; 14]; 22];
    for _ in 0..4 {
        let r = next();
        let (a, b, len) = (
            (r % 12) as usize + 1,
            (r >> 8) as usize % 18 + 2,
            (r >> 16) as usize % 10 + 4,
        );
        for i in 0..len {
            for t in 0..2 {
                let (x, y) = if (r >> 40) & 1 == 1 {
                    (a + t, b + i)
                } else {
                    (a + i, b + t)
                };
                if x < 14 && y < 22 {
                    mask[y][x] = true;
                }
            }
        }
    }
    let mut out = [[0u8; 7]; 11];
    for (y, row) in out.iter_mut().enumerate() {
        for (x, v) in row.iter_mut().enumerate() {
            *v = [(0, 0), (1, 0), (0, 1), (1, 1)]
                .iter()
                .filter(|(dx, dy)| mask[2 * y + dy][2 * x + dx])
                .count() as u8;
        }
    }
    out
}

/// A 4:2:0 frame with `luma` (tightly packed, `width × height`) and neutral chroma.
pub fn luma_frame(
    width: usize,
    height: usize,
    luma: &[u8],
    pts: cm::Time,
) -> Result<arc::R<cm::SampleBuf>> {
    let mut pixels = cv::PixelBuf::new(width, height, cv::PixelFormat::_420V, None)
        .map_err(|e| os_err("pixel buffer", e))?;
    // SAFETY: the buffer is unlocked again below and not shared yet.
    unsafe { pixels.lock_base_addr(Default::default()) }
        .result()
        .map_err(|e| os_err("lock", e))?;
    for plane in 0..pixels.plane_count() {
        let rows = pixels.plane_height(plane);
        let stride = pixels.plane_bytes_per_row(plane);
        let base = pixels.plane_base_address(plane) as *mut u8;
        for y in 0..rows {
            // SAFETY: the plane is locked and `stride * rows` bytes long.
            let row = unsafe { std::slice::from_raw_parts_mut(base.add(y * stride), stride) };
            if plane == 0 {
                row[..width].copy_from_slice(&luma[y * width..(y + 1) * width]);
            } else {
                row.fill(128);
            }
        }
    }
    // SAFETY: matches the lock above.
    let _ = unsafe { pixels.unlock_lock_base_addr(Default::default()) };
    let desc = cm::VideoFormatDesc::with_image_buf(&pixels).map_err(|e| os_err("format", e))?;
    let timing = cm::SampleTimingInfo {
        duration: cm::Time::invalid(),
        pts,
        dts: cm::Time::invalid(),
    };
    cm::SampleBuf::with_image_buf(&pixels, true, None, std::ptr::null(), &desc, &timing)
        .map_err(|e| os_err("sample buffer", e))
}

/// Interleaved float stereo sine tone, `frames` frames long at `rate` Hz.
pub fn audio_chunk(
    rate: f64,
    first_frame: u64,
    frames: usize,
    pts: cm::Time,
) -> Result<arc::R<cm::SampleBuf>> {
    let asbd = cat::audio::StreamBasicDesc {
        sample_rate: rate,
        format: cat::AudioFormat::LINEAR_PCM,
        format_flags: cat::AudioFormatFlags::IS_FLOAT | cat::AudioFormatFlags::IS_PACKED,
        bytes_per_packet: 8,
        frames_per_packet: 1,
        bytes_per_frame: 8,
        channels_per_frame: 2,
        bits_per_channel: 32,
        reserved: 0,
    };
    let desc = cm::AudioFormatDesc::with_asbd(&asbd).map_err(|e| os_err("audio format", e))?;
    let mut block =
        cm::BlockBuf::with_mem_block(frames * 8).map_err(|e| os_err("block buffer", e))?;
    block
        .assure_block_mem()
        .map_err(|e| os_err("block memory", e))?;
    {
        let bytes = block.as_mut_slice().map_err(|e| os_err("block slice", e))?;
        for i in 0..frames {
            let t = (first_frame + i as u64) as f64 / rate;
            let v = ((t * 440.0 * std::f64::consts::TAU).sin() * 0.2) as f32;
            bytes[i * 8..i * 8 + 4].copy_from_slice(&v.to_ne_bytes());
            bytes[i * 8 + 4..i * 8 + 8].copy_from_slice(&v.to_ne_bytes());
        }
    }
    let timing = cm::SampleTimingInfo {
        duration: cm::Time::new(1, rate as i32),
        pts,
        dts: cm::Time::invalid(),
    };
    let mut out = None;
    // SAFETY: one timing entry and one sample size entry describe `frames` packed samples.
    unsafe {
        cm::SampleBuf::create_in(
            None,
            Some(&block),
            true,
            None,
            std::ptr::null(),
            Some(&desc),
            frames as _,
            1,
            &timing,
            1,
            &8usize,
            &mut out,
        )
    }
    .map_err(|e| os_err("audio sample buffer", e))?;
    out.ok_or_else(|| Error::Platform("audio sample buffer".into()))
}
