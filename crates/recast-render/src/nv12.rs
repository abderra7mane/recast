//! RGBA or BGRA to NV12 with BT.709 coefficients and full range (Y and chroma use
//! all of 0..=255), for streaming rendered frames to the preview.

use crate::PixelFormat;

/// Size of an NV12 image: a `width × height` Y plane followed by an interleaved
/// UV plane at half the width and height. Both sides must be even.
pub fn len(width: u32, height: u32) -> usize {
    let luma = width as usize * height as usize;
    luma + luma / 2
}

const Y: [i32; 3] = [13_933, 46_871, 4_732];
const CB: [i32; 3] = [-7_509, -25_259, 32_768];
const CR: [i32; 3] = [32_768, -29_763, -3_005];

fn dot(c: [i32; 3], r: i32, g: i32, b: i32) -> i32 {
    c[0] * r + c[1] * g + c[2] * b
}

/// Converts `width × height` pixels whose rows start `stride` bytes apart into
/// `out`, which must be [`len`] bytes. Chroma is taken from the average of each
/// 2×2 block. Alpha is ignored.
pub fn convert(
    src: &[u8],
    stride: usize,
    width: u32,
    height: u32,
    format: PixelFormat,
    out: &mut [u8],
) {
    assert!(
        width.is_multiple_of(2) && height.is_multiple_of(2),
        "NV12 needs even sides"
    );
    assert!(out.len() >= len(width, height), "NV12 buffer too small");
    let (w, h) = (width as usize, height as usize);
    let (ri, bi) = match format {
        PixelFormat::Rgba8 => (0, 2),
        PixelFormat::Bgra8 => (2, 0),
    };
    let (luma, chroma) = out.split_at_mut(w * h);
    for y in (0..h).step_by(2) {
        let rows = [
            &src[y * stride..y * stride + w * 4],
            &src[(y + 1) * stride..(y + 1) * stride + w * 4],
        ];
        let uv = &mut chroma[(y / 2) * w..(y / 2) * w + w];
        for x in (0..w).step_by(2) {
            let (mut rs, mut gs, mut bs) = (0, 0, 0);
            for (dy, row) in rows.iter().enumerate() {
                for dx in 0..2 {
                    let px = &row[(x + dx) * 4..(x + dx) * 4 + 4];
                    let (r, g, b) = (px[ri] as i32, px[1] as i32, px[bi] as i32);
                    luma[(y + dy) * w + x + dx] = ((dot(Y, r, g, b) + (1 << 15)) >> 16) as u8;
                    rs += r;
                    gs += g;
                    bs += b;
                }
            }
            let chroma = |c: [i32; 3]| {
                ((dot(c, rs, gs, bs) + (128 << 18) + (1 << 17)) >> 18).clamp(0, 255) as u8
            };
            uv[x] = chroma(CB);
            uv[x + 1] = chroma(CR);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
        rgba.repeat((width * height) as usize)
    }

    fn convert_solid(rgba: [u8; 4]) -> (u8, u8, u8) {
        let src = solid(4, 2, rgba);
        let mut out = vec![0; len(4, 2)];
        convert(&src, 16, 4, 2, PixelFormat::Rgba8, &mut out);
        assert!(out[..8].iter().all(|v| *v == out[0]));
        assert_eq!(out[8..], [out[8], out[9], out[8], out[9]]);
        (out[0], out[8], out[9])
    }

    #[test]
    fn known_colors() {
        assert_eq!(convert_solid([0, 0, 0, 255]), (0, 128, 128));
        assert_eq!(convert_solid([255, 255, 255, 255]), (255, 128, 128));
        // Y = 0.2126·255, Cb = 128 − 0.1146·255, Cr = 128 + 0.5·255 (clamped)
        assert_eq!(convert_solid([255, 0, 0, 255]), (54, 99, 255));
        assert_eq!(convert_solid([0, 255, 0, 255]), (182, 30, 12));
        assert_eq!(convert_solid([0, 0, 255, 255]), (18, 255, 116));
        assert_eq!(convert_solid([128, 128, 128, 0]), (128, 128, 128));
    }

    #[test]
    fn bgra_matches_rgba() {
        let rgba: Vec<u8> = (0..4 * 4 * 4).map(|i| (i * 37 % 256) as u8).collect();
        let bgra: Vec<u8> = rgba
            .chunks_exact(4)
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();
        let mut a = vec![0; len(4, 4)];
        let mut b = vec![0; len(4, 4)];
        convert(&rgba, 16, 4, 4, PixelFormat::Rgba8, &mut a);
        convert(&bgra, 16, 4, 4, PixelFormat::Bgra8, &mut b);
        assert_eq!(a, b);
    }

    #[test]
    fn chroma_averages_each_block_and_respects_stride() {
        // 4×2 with 4 bytes of row padding: left block red and blue, right block white.
        let red = [255, 0, 0, 255];
        let blue = [0, 0, 255, 255];
        let white = [255, 255, 255, 255];
        let mut src = Vec::new();
        for row in [[red, blue, white, white], [blue, red, white, white]] {
            for px in row {
                src.extend_from_slice(&px);
            }
            src.extend_from_slice(&[9, 9, 9, 9]);
        }
        let mut out = vec![0; len(4, 2)];
        convert(&src, 20, 4, 2, PixelFormat::Rgba8, &mut out);
        assert_eq!(out[..4], [54, 18, 255, 255]);
        assert_eq!(out[4..8], [18, 54, 255, 255]);
        // Average of red and blue is (127.5, 0, 127.5).
        let (r, b) = (127.5_f64, 127.5_f64);
        let cb = (128.0 - 0.114_572 * r + 0.5 * b).round() as u8;
        let cr = (128.0 + 0.5 * r - 0.045_847 * b).round() as u8;
        assert_eq!(out[8..], [cb, cr, 128, 128]);
    }

    #[test]
    fn round_trip_stays_close() {
        let inverse = |y: u8, cb: u8, cr: u8| {
            let (y, cb, cr) = (y as f64, cb as f64 - 128.0, cr as f64 - 128.0);
            [
                y + 1.5748 * cr,
                y - 0.1873 * cb - 0.4681 * cr,
                y + 1.8556 * cb,
            ]
        };
        for rgb in [[200, 120, 40], [10, 200, 90], [90, 90, 250], [128, 64, 32]] {
            let (y, cb, cr) = convert_solid([rgb[0], rgb[1], rgb[2], 255]);
            let back = inverse(y, cb, cr);
            for (a, b) in rgb.iter().zip(back) {
                assert!((*a as f64 - b).abs() <= 2.0, "{rgb:?} -> {back:?}");
            }
        }
    }
}
