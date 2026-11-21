use yorimichi_core::photos::{jpeg_bytes, make_thumbnail};

fn write_png(path: &std::path::Path, w: u32, h: u32) {
    image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255])).save(path).unwrap();
}

#[test]
fn thumbnail_fits_long_edge_and_is_jpeg() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("big.png");
    write_png(&src, 800, 400);
    let dest = dir.path().join("cache").join("sub").join("t.jpg");
    make_thumbnail(&src, &dest, 200).unwrap();

    assert!(!dest.with_extension("tmp").exists());
    let bytes = std::fs::read(&dest).unwrap();
    assert_eq!(image::guess_format(&bytes).unwrap(), image::ImageFormat::Jpeg);
    let img = image::load_from_memory(&bytes).unwrap();
    assert_eq!((img.width(), img.height()), (200, 100));
}

#[test]
fn small_images_are_not_upscaled() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("small.png");
    write_png(&src, 64, 32);
    let bytes = jpeg_bytes(&src, 200, 80).unwrap();
    let img = image::load_from_memory(&bytes).unwrap();
    assert_eq!((img.width(), img.height()), (64, 32));
}

#[test]
fn undecodable_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("bad.png");
    std::fs::write(&src, b"nope").unwrap();
    assert!(jpeg_bytes(&src, 100, 80).is_err());
}

/// Manual probe: `PROBE_SRC=<photo> PROBE_OUT=<jpg> cargo test -- --ignored probe_real_file`.
#[test]
#[ignore]
fn probe_real_file() {
    let src = std::env::var("PROBE_SRC").unwrap();
    let out = std::env::var("PROBE_OUT").unwrap();
    make_thumbnail(std::path::Path::new(&src), std::path::Path::new(&out), 400).unwrap();
}
