//! Usage: shell_thumb_probe <file> <out.jpg>. Prints attributes before/after a shell thumbnail request.
use std::os::windows::fs::MetadataExt;
use std::path::Path;
use std::time::Instant;

fn attrs(p: &Path) -> String {
    match std::fs::metadata(p) {
        Ok(md) => format!(
            "0x{:x} online_only={}",
            md.file_attributes(),
            vrclogger_core::photos::is_online_only(&md)
        ),
        Err(e) => format!("error: {e}"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (file, out) = (Path::new(&args[1]), Path::new(&args[2]));
    println!("before: {}", attrs(file));
    let t = Instant::now();
    let jpg = vrclogger_lib::shell_thumb::shell_thumbnail_jpeg(file, 400);
    println!("elapsed: {:?}", t.elapsed());
    println!("after:  {}", attrs(file));
    match jpg {
        Some(b) => {
            std::fs::write(out, &b).unwrap();
            let img = image::load_from_memory_with_format(&b, image::ImageFormat::Jpeg).unwrap();
            println!("thumbnail: {} bytes, {}x{}", b.len(), img.width(), img.height());
        }
        None => println!("thumbnail: none"),
    }
}
