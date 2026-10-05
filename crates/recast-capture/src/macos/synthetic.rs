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
