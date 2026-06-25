use std::time::SystemTime;

/// Returns true if nth bit in array is 1.
#[allow(dead_code)]
pub(crate) fn test_bit(n: u16, array: &[u8]) -> bool {
    (array[(n / 8) as usize] >> (n % 8)) & 1 != 0
}

#[cfg(not(target_arch = "wasm32"))]
pub fn time_now() -> SystemTime {
    SystemTime::now()
}

#[cfg(target_arch = "wasm32")]
pub fn time_now() -> SystemTime {
    use js_sys::Date;
    use std::time::Duration;

    let offset = Duration::from_millis(Date::now() as u64);
    SystemTime::UNIX_EPOCH + offset
}

/// For WASM this always returns 0, as monotonic time is not supported.
///
/// There is no host clock on WASM, so monotonic time is not available
/// but this is implemented to prevent errors when creating events on
/// WASM targets.
#[cfg(target_arch = "wasm32")]
pub fn monotonic_now_ns() -> u64 {
    0
}

/// Current monotonic time since system startup in nanoseconds.
///
/// On unix systems this reads `CLOCK_UPTIME_RAW` — the same clock domain
/// as processed frame timestamps (mach absolute time). Uptime raw is the
/// correct clock to fetch from as it is not affected by system sleep.
#[cfg(target_os = "macos")]
pub fn monotonic_now_ns() -> u64 {
    clock_gettime_ns(libc::CLOCK_UPTIME_RAW)
}

/// Current monotonic time since system startup in nanoseconds.
///
/// On unix systems this reads `CLOCK_MONOTONIC_RAW` — the same clock domain
/// as processed frame timestamps (mach absolute time).
#[cfg(target_os = "linux")]
pub fn monotonic_now_ns() -> u64 {
    clock_gettime_ns(libc::CLOCK_MONOTONIC_RAW)
}

/// Current monotonic time since system startup in nanoseconds.
///
/// On Windows this reads [`QueryUnbiasedInterruptTimePrecise`], the closest
/// analogue to Apple's `CLOCK_UPTIME_RAW`: a high-resolution interrupt-time
/// count that is monotonically increasing and excludes time the system spends
/// asleep or hibernating.
///
/// The value is reported in 100-nanosecond units, so we scale by 100.
///
/// ## Reference
/// https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/nf-realtimeapiset-queryunbiasedinterrupttimeprecise
#[cfg(target_os = "windows")]
pub fn monotonic_now_ns() -> u64 {
    use windows::Win32::System::WindowsProgramming::QueryUnbiasedInterruptTimePrecise;

    let mut ticks_100ns: u64 = 0;
    unsafe { QueryUnbiasedInterruptTimePrecise(&mut ticks_ns) };
    ticks_100ns * 100
}

/// Get the current time in nanoseconds for the given clock ID.
///
/// ## Reference
/// https://www.unix.com/man-page/mojave/3/clock_gettime/
#[cfg(unix)]
fn clock_gettime_ns(clock_id: libc::clockid_t) -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let ret = unsafe { libc::clock_gettime(clock_id, &mut ts) };
    debug_assert_eq!(ret, 0, "clock_gettime failed");
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}
