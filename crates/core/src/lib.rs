//! VrcLogger core: the Tauri-independent library behind the app.
//!
//! * [`parsing`] turns VRChat `output_log_*.txt` text into structured events.
//! * [`data`] owns the SQLite schema ([`data::db`]), the diff importer
//!   ([`data::import`]), the read side ([`data::query`]) and its row types ([`data::dtos`]).
//!   It also holds the user-written library ([`data::library`]: player memos and tags, world
//!   notes, albums, timeline and stats views) and the photo queries and metadata planning.
//! * [`photos`] scans photo folders, reads capture time and VRChat XMP, makes thumbnails and
//!   writes metadata into photos on request.
//! * [`export`] writes the self-contained HTML viewer for other PCs and the shareable
//!   album / day story pages.
//!
//! # Errors
//! Every fallible public function returns [`Result`], i.e. `Result<T, Error>`.
//! [`Error`] implements `Display` (English, for logs); a cause the user can run into is a
//! variant ([`PhotoError`], [`LibraryError`]) so the app can word it in Japanese.
//!
//! # Timestamps
//! Timestamps are `chrono::NaiveDateTime` (local time, as in the log) in Rust and are
//! stored in SQLite as `yyyy-MM-dd HH:mm:ss` text (see [`data::db::ts`]).
//! Date filters take `Option<chrono::NaiveDate>` (inclusive on both ends).

pub mod data;
pub mod export;
pub mod parsing;
pub mod photos;

mod error;

pub use error::{Error, LibraryError, NameKind, PhotoError, Result};
