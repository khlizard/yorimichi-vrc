//! Photo library: file scanning, metadata extraction (PNG XMP / file name / mtime) and thumbnails.
//!
//! Photo files are only ever read, never modified.

pub mod meta;
pub mod scan;
pub mod thumbs;

pub use meta::{parse_xmp, read_meta, read_png_info, time_from_name, PhotoMeta, PngInfo, TimeSource, XmpInfo};
pub use scan::{is_online_only, relink_visits, scan_folders, ScanSummary, PHOTO_EXTENSIONS};
pub use thumbs::{jpeg_bytes, make_thumbnail};
