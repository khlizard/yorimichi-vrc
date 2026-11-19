//! VrcLogger core: a Tauri-independent port of the C# `VrcLogger.Core` library.
//!
//! * [`parsing`] turns VRChat `output_log_*.txt` text into structured events.
//! * [`data`] owns the SQLite schema ([`data::db`]), the diff importer
//!   ([`data::import`]), the read side ([`data::query`]) and its row types ([`data::dtos`]).
//! * [`export`] writes the self-contained HTML viewer for other PCs.
//!
//! # Errors
//! Every fallible public function returns [`Result`], i.e. `Result<T, Error>`.
//! [`Error`] implements `Display`, so a Tauri command can map it with
//! `.map_err(|e| e.to_string())`.
//!
//! # Timestamps
//! Timestamps are `chrono::NaiveDateTime` (local time, as in the log) in Rust and are
//! stored in SQLite as `yyyy-MM-dd HH:mm:ss` text (see [`data::db::ts`]).
//! Date filters take `Option<chrono::NaiveDate>` (inclusive on both ends).

pub mod data;
pub mod export;
pub mod parsing;
pub mod photos;

/// Error type returned by the public API.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The embedded HTML template is malformed (e.g. placeholder missing or duplicated).
    #[error("{0}")]
    Template(String),
    /// A photo could not be decoded or a thumbnail could not be encoded.
    #[error("image error: {0}")]
    Image(String),
    /// A caller-supplied value was rejected (e.g. an empty tag or album name).
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;
