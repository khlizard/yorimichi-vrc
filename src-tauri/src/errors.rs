//! Japanese wording of the core library's errors, for everything the user can see (status line,
//! error dialogs, the failure list of a metadata run). `Display` of the errors stays English and is
//! meant for logs.

use yorimichi_core::{Error, LibraryError, NameKind, PhotoError};

/// The text shown to the user for a core error. SQLite and I/O errors keep the original message
/// after a Japanese description: `（説明）: （元のメッセージ）`.
pub fn user_message(e: &Error) -> String {
    match e {
        Error::Sqlite(e) => format!("データベースの処理でエラーが発生しました: {e}"),
        Error::Io(e) => format!("ファイルの読み書きでエラーが発生しました: {e}"),
        Error::Template(e) => format!("書き出し用のひな形が正しくありません: {e}"),
        Error::Image(e) => format!("画像を処理できませんでした: {e}"),
        Error::Photo(p) => photo_message(p),
        Error::Library(l) => library_message(l),
    }
}

/// The text for one failed photo (also the entries of a metadata run's failure list).
pub fn photo_message(e: &PhotoError) -> String {
    match e {
        PhotoError::NotAFile => "通常のファイルではありません".to_string(),
        PhotoError::OnlineOnly => "オンライン専用のファイルです (先にダウンロードが必要です)".to_string(),
        PhotoError::ReadOnly => "読み取り専用のファイルです".to_string(),
        PhotoError::UnsupportedFormat => "対応していない画像形式です (PNG / JPEG のみ)".to_string(),
        PhotoError::MalformedPng(_) => "PNG ファイルの構造が正しくありません".to_string(),
        PhotoError::MalformedJpeg => "JPEG ファイルの構造が正しくありません".to_string(),
        PhotoError::XmpTooLargeForPng => "メタデータが大きすぎて PNG に書き込めません".to_string(),
        PhotoError::XmpTooLargeForJpeg { size, max } => {
            format!("メタデータが大きすぎて JPEG に書き込めません ({size} / {max} バイト)")
        }
        PhotoError::AlreadyHasXmp => "すでにメタデータが含まれています".to_string(),
        PhotoError::NoFileName => "ファイル名を取得できません".to_string(),
        PhotoError::Verification(_) => "書き込み結果の確認に失敗したため、ファイルは変更していません".to_string(),
        PhotoError::ChangedWhileProcessing => "処理中にファイルが変更されました".to_string(),
        PhotoError::TimestampsNotRestored(e) => {
            format!("ファイルは更新されましたが、更新日時を元に戻せませんでした: {e}")
        }
        PhotoError::BadTakenAt { value, reason } => format!("撮影日時を読み取れません ({value}): {reason}"),
        PhotoError::DownloadFailed(e) => format!("ファイルをダウンロードできませんでした: {e}"),
        PhotoError::LibraryRefreshFailed(e) => format!("ライブラリの情報を更新できませんでした: {e}"),
        PhotoError::UpdatedButLibraryRefreshFailed(e) => {
            format!("ファイルは更新されましたが、ライブラリの情報を更新できませんでした: {e}")
        }
        PhotoError::Io(e) => format!("ファイルの読み書きでエラーが発生しました: {e}"),
        PhotoError::Database(e) => format!("データベースの処理でエラーが発生しました: {e}"),
        PhotoError::Other(e) => format!("エラーが発生しました: {e}"),
    }
}

fn library_message(e: &LibraryError) -> String {
    match e {
        LibraryError::EmptyName(NameKind::Tag) => "タグの名前を入力してください".to_string(),
        LibraryError::EmptyName(NameKind::Album) => "アルバムの名前を入力してください".to_string(),
        LibraryError::AlbumNotFound => "アルバムが見つかりません".to_string(),
        LibraryError::NoPhotosInVisit => "このセッションには写真がありません".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_japanese(s: &str) -> bool {
        s.chars().any(|c| ('\u{3040}'..='\u{30ff}').contains(&c) || ('\u{4e00}'..='\u{9fff}').contains(&c))
    }

    #[test]
    fn library_errors_keep_their_old_wording() {
        assert_eq!(user_message(&LibraryError::NoPhotosInVisit.into()), "このセッションには写真がありません");
        assert_eq!(user_message(&LibraryError::AlbumNotFound.into()), "アルバムが見つかりません");
        assert!(user_message(&LibraryError::EmptyName(NameKind::Tag).into()).contains("タグ"));
        assert!(user_message(&LibraryError::EmptyName(NameKind::Album).into()).contains("アルバム"));
    }

    #[test]
    fn sqlite_and_io_errors_keep_the_original_message() {
        let io = std::io::Error::other("disk exploded");
        let msg = user_message(&Error::Io(io));
        assert!(msg.starts_with("ファイルの読み書きでエラーが発生しました: "), "{msg}");
        assert!(msg.ends_with("disk exploded"), "{msg}");
        let db = rusqlite::Error::InvalidQuery;
        let msg = user_message(&Error::Sqlite(db));
        assert!(msg.starts_with("データベースの処理でエラーが発生しました: "), "{msg}");
        assert!(msg.len() > "データベースの処理でエラーが発生しました: ".len());
    }

    #[test]
    fn every_photo_error_is_worded_in_japanese() {
        let all = [
            PhotoError::NotAFile,
            PhotoError::OnlineOnly,
            PhotoError::ReadOnly,
            PhotoError::UnsupportedFormat,
            PhotoError::MalformedPng("x"),
            PhotoError::MalformedJpeg,
            PhotoError::XmpTooLargeForPng,
            PhotoError::XmpTooLargeForJpeg { size: 2, max: 1 },
            PhotoError::AlreadyHasXmp,
            PhotoError::NoFileName,
            PhotoError::Verification("x"),
            PhotoError::ChangedWhileProcessing,
            PhotoError::TimestampsNotRestored("x".into()),
            PhotoError::BadTakenAt { value: "v".into(), reason: "r".into() },
            PhotoError::DownloadFailed("x".into()),
            PhotoError::LibraryRefreshFailed("x".into()),
            PhotoError::UpdatedButLibraryRefreshFailed("x".into()),
            PhotoError::Io("x".into()),
            PhotoError::Database("x".into()),
            PhotoError::Other("x".into()),
        ];
        for e in all {
            let msg = photo_message(&e);
            assert!(has_japanese(&msg), "{e:?}: {msg}");
            assert_eq!(user_message(&Error::Photo(e.clone())), msg);
        }
    }
}
