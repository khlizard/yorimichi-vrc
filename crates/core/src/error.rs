//! The error types of the public API.
//!
//! `Display` texts are English: they are for logs and developers. The app turns an [`Error`]
//! into the Japanese text the user sees (see `user_message` in the Tauri crate), so a cause that
//! can reach the user is a variant here, not a free-form string.

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
    /// A photo file or its metadata could not be handled.
    #[error("{0}")]
    Photo(#[from] PhotoError),
    /// A caller-supplied value or request for the user-written library was rejected.
    #[error("{0}")]
    Library(#[from] LibraryError),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Why a photo file (or its metadata) could not be read, written or refreshed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PhotoError {
    #[error("not a regular file")]
    NotAFile,
    #[error("the file is online-only; it would have to be downloaded first")]
    OnlineOnly,
    #[error("the file is read-only")]
    ReadOnly,
    #[error("unsupported format")]
    UnsupportedFormat,
    /// A PNG that does not parse; the text says where.
    #[error("malformed PNG: {0}")]
    MalformedPng(&'static str),
    #[error("malformed JPEG")]
    MalformedJpeg,
    #[error("XMP too large for a PNG chunk")]
    XmpTooLargeForPng,
    #[error("XMP is too large for a JPEG segment ({size} > {max} bytes)")]
    XmpTooLargeForJpeg { size: usize, max: usize },
    #[error("the file already contains XMP metadata")]
    AlreadyHasXmp,
    #[error("path has no file name")]
    NoFileName,
    /// A check after (or before) writing failed; nothing was changed. The text says which check.
    #[error("verification failed: {0}")]
    Verification(&'static str),
    #[error("the file changed while it was being processed")]
    ChangedWhileProcessing,
    #[error("the file was updated but its timestamps could not be restored: {0}")]
    TimestampsNotRestored(String),
    #[error("bad taken_at '{value}': {reason}")]
    BadTakenAt { value: String, reason: String },
    #[error("could not download the file: {0}")]
    DownloadFailed(String),
    #[error("the library could not be refreshed: {0}")]
    LibraryRefreshFailed(String),
    #[error("the file was updated but the library could not be refreshed: {0}")]
    UpdatedButLibraryRefreshFailed(String),
    /// An [`Error::Io`] seen while handling a photo (kept as text so the enum stays `Clone + Eq`).
    #[error("I/O error: {0}")]
    Io(String),
    /// An [`Error::Sqlite`] seen while handling a photo.
    #[error("database error: {0}")]
    Database(String),
    /// Any other [`Error`] seen while handling a photo.
    #[error("{0}")]
    Other(String),
}

impl From<Error> for PhotoError {
    fn from(e: Error) -> Self {
        match e {
            Error::Photo(p) => p,
            Error::Io(e) => PhotoError::Io(e.to_string()),
            Error::Sqlite(e) => PhotoError::Database(e.to_string()),
            other => PhotoError::Other(other.to_string()),
        }
    }
}

/// What a name belongs to, for [`LibraryError::EmptyName`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    Tag,
    Album,
}

impl std::fmt::Display for NameKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            NameKind::Tag => "tag",
            NameKind::Album => "album",
        })
    }
}

/// A request to the user-written library (tags, albums) that cannot be carried out.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LibraryError {
    #[error("{0} name must not be empty")]
    EmptyName(NameKind),
    #[error("album not found")]
    AlbumNotFound,
    #[error("the visit has no photos")]
    NoPhotosInVisit,
}
