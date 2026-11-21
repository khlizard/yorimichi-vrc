//! Cloud placeholders (Dropbox / OneDrive "online-only" files).
//!
//! Only the explicit "download and write metadata" run uses this: it downloads a file by
//! reading it. The file then stays local: Dropbox ignores requests from other apps to make a
//! file online-only again (`attrib +U` is reverted, `CfDehydratePlaceholder` never returns).

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use super::scan::is_online_only;

/// Downloads an online-only file by reading it to the end (the sync client fills it in on
/// access). Returns an error if it is still a placeholder afterwards.
pub fn hydrate(path: &Path) -> io::Result<()> {
    let mut f = File::open(path)?;
    let mut buf = vec![0u8; 1 << 20];
    while f.read(&mut buf)? > 0 {}
    drop(f);
    if is_online_only(&std::fs::metadata(path)?) {
        return Err(io::Error::other("the file is still online-only after reading it"));
    }
    Ok(())
}

/// Free bytes available to this user on the volume holding `path` (`None` if unknown).
pub fn available_bytes(path: &Path) -> Option<u64> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let dir = if path.is_dir() { path } else { path.parent()? };
        let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let mut free = 0u64;
        // SAFETY: `wide` is NUL-terminated; the other out-pointers may be null.
        let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
        (ok != 0).then_some(free)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}
