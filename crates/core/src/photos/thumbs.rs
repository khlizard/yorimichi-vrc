//! JPEG thumbnails. Sources are only read; output goes to the given destination.

use std::path::Path;

use image::{codecs::jpeg::JpegEncoder, DynamicImage, ExtendedColorType, RgbImage};

use crate::{Error, Result};

/// Background used where the source is transparent.
const BACKGROUND: [u8; 3] = [0x1b, 0x1b, 0x1f];

fn img_err(e: impl std::fmt::Display) -> Error {
    Error::Image(e.to_string())
}

/// Decodes a photo. JPEG goes through `jpeg-decoder`: the decoder built into `image` renders
/// baseline files with one scan per component (non-interleaved) as a green, striped picture.
fn decode(src: &Path) -> Result<DynamicImage> {
    let data = std::fs::read(src)?;
    if data.starts_with(&[0xFF, 0xD8]) {
        if let Some(img) = decode_jpeg(&data) {
            return Ok(img);
        }
    }
    image::ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()?
        .decode()
        .map_err(img_err)
}

/// `None` for pixel formats this path does not handle (CMYK, 16-bit) or on a decode error;
/// the caller then falls back to `image`.
fn decode_jpeg(data: &[u8]) -> Option<DynamicImage> {
    let mut decoder = jpeg_decoder::Decoder::new(data);
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    let (w, h) = (u32::from(info.width), u32::from(info.height));
    match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => RgbImage::from_raw(w, h, pixels).map(DynamicImage::ImageRgb8),
        jpeg_decoder::PixelFormat::L8 => image::GrayImage::from_raw(w, h, pixels).map(DynamicImage::ImageLuma8),
        _ => None,
    }
}

fn render(src: &Path, max_edge: u32) -> Result<RgbImage> {
    let img = decode(src)?;
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
