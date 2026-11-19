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
