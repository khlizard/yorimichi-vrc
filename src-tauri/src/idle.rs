//! System idle / CPU load probes used to decide when background maintenance may run.

use std::time::Duration;

/// Seconds since the last keyboard/mouse input (0 when it cannot be determined, i.e. "never idle").
#[cfg(windows)]
pub fn idle_seconds() -> u64 {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    // SAFETY: `info` is a valid, correctly sized LASTINPUTINFO.
    if unsafe { GetLastInputInfo(&mut info) } == 0 {
        return 0;
    }
    // Both are 32-bit tick counts; wrapping_sub stays correct across the 49-day rollover.
    let now = unsafe { GetTickCount() };
    u64::from(now.wrapping_sub(info.dwTime)) / 1000
}

#[cfg(not(windows))]
pub fn idle_seconds() -> u64 {
    0
}

/// True if a process with this executable name (e.g. `VRChat.exe`, compared without case) is
/// running. False when the process list cannot be read.
#[cfg(windows)]
pub fn process_running(exe_name: &str) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };

    // SAFETY: plain snapshot of the process list; the handle is closed below.
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap == INVALID_HANDLE_VALUE {
        return false;
    }
    // SAFETY: PROCESSENTRY32W is plain data; dwSize is set as the API requires.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut found = false;
    // SAFETY: `snap` is a valid snapshot handle and `entry` a valid, sized PROCESSENTRY32W.
    let mut more = unsafe { Process32FirstW(snap, &mut entry) } != 0;
    while more {
        let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case(exe_name) {
            found = true;
            break;
        }
        // SAFETY: as above.
        more = unsafe { Process32NextW(snap, &mut entry) } != 0;
    }
    // SAFETY: `snap` came from CreateToolhelp32Snapshot and is closed once.
    unsafe { CloseHandle(snap) };
    found
}

#[cfg(not(windows))]
pub fn process_running(_exe_name: &str) -> bool {
    false
}

#[cfg(windows)]
fn cpu_times() -> Option<(u64, u64, u64)> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::GetSystemTimes;

    let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut idle, mut kernel, mut user) = (zero, zero, zero);
    // SAFETY: all three pointers refer to valid FILETIME values.
    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
        return None;
    }
    let v = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    Some((v(idle), v(kernel), v(user)))
}

/// Fraction (0.0..=1.0) of CPU time spent busy while sleeping for `sample`. Blocks the calling thread.
/// Returns 1.0 when it cannot be measured, so callers treat the machine as busy.
#[cfg(windows)]
pub fn cpu_busy_fraction(sample: Duration) -> f32 {
    let Some(a) = cpu_times() else { return 1.0 };
    std::thread::sleep(sample);
    let Some(b) = cpu_times() else { return 1.0 };
    let idle = b.0.saturating_sub(a.0);
    // Kernel time already includes idle time.
    let total = b.1.saturating_sub(a.1) + b.2.saturating_sub(a.2);
    if total == 0 {
        return 1.0;
    }
    (1.0 - idle as f64 / total as f64).clamp(0.0, 1.0) as f32
}

#[cfg(not(windows))]
pub fn cpu_busy_fraction(_sample: Duration) -> f32 {
    1.0
}
