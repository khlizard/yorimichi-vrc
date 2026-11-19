//! JPEG thumbnails. Sources are only read; output goes to the given destination.

use std::path::Path;

use image::{codecs::jpeg::JpegEncoder, DynamicImage, ExtendedColorType, RgbImage};

use crate::{Error, Result};

/// Background used where the source is transparent.
const BACKGROUND: [u8; 3] = [0x1b, 0x1b, 0x1f];

fn img_err(e: impl std::fmt::Display) -> Error {
    Error::Image(e.to_string())
}

fn render(src: &Path, max_edge: u32) -> Result<RgbImage> {
    let img = image::ImageReader::open(src)?
        .with_guessed_format()?
        .decode()
        .map_err(img_err)?;
    let img = if img.width() > max_edge || img.height() > max_edge {
        img.thumbnail(max_edge, max_edge)
    } else {
        img
    };
    Ok(flatten(&img))
}

fn flatten(img: &DynamicImage) -> RgbImage {
    let rgba = img.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = u32::from(p[3]);
        let mix = |c: u8, bg: u8| ((u32::from(c) * a + u32::from(bg) * (255 - a) + 127) / 255) as u8;
        out.put_pixel(
            x,
            y,
            image::Rgb([mix(p[0], BACKGROUND[0]), mix(p[1], BACKGROUND[1]), mix(p[2], BACKGROUND[2])]),
        );
    }
    out
}

/// Returns the thumbnail as JPEG bytes (long side at most `max_edge`, never upscaled).
pub fn jpeg_bytes(src: &Path, max_edge: u32, quality: u8) -> Result<Vec<u8>> {
    let rgb = render(src, max_edge)?;
    let mut buf = Vec::new();
    JpegEncoder::new_with_quality(&mut buf, quality)
        .encode(rgb.as_raw(), rgb.width(), rgb.height(), ExtendedColorType::Rgb8)
        .map_err(img_err)?;
    Ok(buf)
}

/// Writes the thumbnail to `dest` atomically (temp file + rename), creating the parent directory.
pub fn make_thumbnail(src: &Path, dest: &Path, max_edge: u32) -> Result<()> {
    let bytes = jpeg_bytes(src, max_edge, 82)?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    if let Err(e) = std::fs::rename(&tmp, dest) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}
