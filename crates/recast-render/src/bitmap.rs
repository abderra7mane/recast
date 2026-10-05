use std::path::Path;

use crate::{Error, Result};

/// RGBA pixels with premultiplied alpha, tightly packed.
#[derive(Debug, Clone, PartialEq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rgba {
    pub fn load(path: &Path) -> Result<Self> {
        let image = image::open(path)
            .map_err(|e| Error::Image(format!("{}: {e}", path.display())))?
            .to_rgba8();
        Ok(Self::from_straight(
            image.width(),
            image.height(),
            image.into_raw(),
        ))
    }

    /// Converts straight (non-premultiplied) RGBA.
    pub fn from_straight(width: u32, height: u32, mut pixels: Vec<u8>) -> Self {
        for px in pixels.chunks_exact_mut(4) {
            let a = px[3] as u32;
            for c in &mut px[..3] {
                *c = ((*c as u32 * a + 127) / 255) as u8;
            }
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    /// Half-size copies down to 1×1, each averaging 2×2 pixels.
    pub fn mip_chain(&self) -> Vec<Rgba> {
        let mut levels = vec![self.clone()];
        while let Some(last) = levels.last()
            && (last.width > 1 || last.height > 1)
        {
            let next = last.half();
            levels.push(next);
        }
        levels
    }

    fn half(&self) -> Rgba {
        let (w, h) = ((self.width / 2).max(1), (self.height / 2).max(1));
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        let at = |x: u32, y: u32, c: usize| {
            let x = x.min(self.width - 1);
            let y = y.min(self.height - 1);
            self.pixels[((y * self.width + x) * 4) as usize + c] as u32
        };
        for y in 0..h {
            for x in 0..w {
                for c in 0..4 {
                    let sum = at(2 * x, 2 * y, c)
                        + at(2 * x + 1, 2 * y, c)
                        + at(2 * x, 2 * y + 1, c)
                        + at(2 * x + 1, 2 * y + 1, c);
                    pixels[((y * w + x) * 4) as usize + c] = ((sum + 2) / 4) as u8;
                }
            }
        }
        Rgba {
            width: w,
            height: h,
            pixels,
        }
    }
}

/// Writes tightly packed straight RGBA as a PNG.
pub fn save_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    image::save_buffer(path, rgba, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|e| Error::Image(format!("{}: {e}", path.display())))
}

/// Encodes tightly packed straight RGBA as PNG bytes.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::write_buffer_with_format(
        &mut out,
        rgba,
        width,
        height,
        image::ExtendedColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|e| Error::Image(e.to_string()))?;
    Ok(out.into_inner())
}

/// Reads a PNG as tightly packed straight RGBA.
pub fn load_png(path: &Path) -> Result<(u32, u32, Vec<u8>)> {
    let image = image::open(path)
        .map_err(|e| Error::Image(format!("{}: {e}", path.display())))?
        .to_rgba8();
    Ok((image.width(), image.height(), image.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiplies() {
        let image = Rgba::from_straight(1, 1, vec![200, 100, 50, 128]);
        assert_eq!(image.pixels, vec![100, 50, 25, 128]);
    }

    #[test]
    fn mip_chain_halves_to_one_pixel() {
        let image = Rgba::from_straight(5, 2, vec![255; 5 * 2 * 4]);
        let sizes: Vec<(u32, u32)> = image
            .mip_chain()
            .iter()
            .map(|l| (l.width, l.height))
            .collect();
        assert_eq!(sizes, vec![(5, 2), (2, 1), (1, 1)]);
    }

    #[test]
    fn png_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.png");
        let pixels: Vec<u8> = (0..2 * 3 * 4).map(|i| i as u8 * 10).collect();
        save_png(&path, 2, 3, &pixels).unwrap();
        assert_eq!(load_png(&path).unwrap(), (2, 3, pixels.clone()));
        let bytes = encode_png(2, 3, &pixels).unwrap();
        assert_eq!(&bytes[1..4], b"PNG");
    }
}
