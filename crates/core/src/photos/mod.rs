//! Photo library: file scanning, metadata extraction (PNG XMP / file name / mtime) and thumbnails.
//!
//! Photo files are read-only, with one exception: [`xmp_write`] can insert (and remove again)
//! a single XMP packet, byte for byte and verified, when the user explicitly asks for it.
//! Online-only files are never read, except by that same explicit run when the user allows
//! downloading them ([`cloud`]).

pub mod cloud;
pub mod meta;
pub mod scan;
pub mod thumbs;
pub mod xmp_write;

pub use meta::{parse_xmp, read_jpeg_xmp, read_meta, read_png_info, time_from_name, PhotoMeta, PngInfo, TimeSource, XmpInfo};
pub use scan::{is_online_only, refresh_file, relink_visits, scan_folders, ScanSummary, PHOTO_EXTENSIONS};
pub use thumbs::{jpeg_bytes, make_thumbnail, rgba_pixels};
pub use xmp_write::{
    build_xmp, insert_xmp, remove_our_xmp_from_file, strip_our_xmp, write_xmp_to_file, xmp_presence, XmpFields,
    XmpPresence,
};
