//! Built-in `time` module — wall clock, monotonic clock, sleep,
//! and the calendar conversions backing `libs/std/time.il`'s
//! `DateTime` class.
//!
//! Calendar math is done with Howard Hinnant's `days_from_civil`
//! / `civil_from_days` algorithms — pure integer arithmetic, no
//! external crate. The only OS-touching piece is `local_offset_min`
//! (POSIX `localtime_r` for `tm_gmtoff`, Windows
//! `_localtime64_s` + `_mkgmtime64` delta).
//!
//! strftime subset supported by `__format`:
//!   `%Y %y %m %d %H %I %M %S %L %j %a %A %b %B %p %z %:z %s %% %n %t`
//! Unknown specifiers are emitted verbatim (`%X` → `%X`).

use std::mem::MaybeUninit;
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::arrays::build_i64_array;
use crate::kind::KIND_NONE;
use crate::strings::{cstr_bytes, leak_cstring};

// ---------------------------------------------------------------
// Calendar arithmetic — Howard Hinnant's chrono::civil algorithms.
// ---------------------------------------------------------------

/// Serial days (epoch 1970-01-01) → `(year, month 1-12, day 1-31)`.
/// Works for any `i64` day count without overflow over the
/// realistic range of timestamps.
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y as i32, m as u32, d as u32)
}

/// `(year, month 1-12, day 1-31)` → serial days from 1970-01-01.
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = (if m <= 2 { y - 1 } else { y }) as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m_adj = if m > 2 { m - 3 } else { m + 9 } as u64;
    let doy = (153 * m_adj + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64 - 719468
}

/// Day-of-week: 0=Sunday, 6=Saturday.
fn weekday_from_days(z: i64) -> u32 {
    ((z + 4).rem_euclid(7)) as u32
}

/// Day-of-year, 1-based.
fn day_of_year(year: i32, month: u32, day: u32) -> u32 {
    let start = days_from_civil(year, 1, 1);
    let cur = days_from_civil(year, month, day);
    (cur - start + 1) as u32
}

/// Break `epoch_ms` down at the given UTC offset and return the
/// 9-element fixed layout
/// `[year, month, day, hour, minute, second, ms, weekday, offsetMin]`.
fn break_down(epoch_ms: i64, offset_min: i32) -> [i64; 9] {
    let adjusted = epoch_ms + (offset_min as i64) * 60_000;
    let total_secs = adjusted.div_euclid(1000);
    let sub_ms = adjusted.rem_euclid(1000);
    let days = total_secs.div_euclid(86_400);
    let sod = total_secs.rem_euclid(86_400);
    let hour = sod / 3600;
    let minute = (sod / 60) % 60;
    let second = sod % 60;
    let (y, m, d) = civil_from_days(days);
    let wd = weekday_from_days(days);
    [
        y as i64,
        m as i64,
        d as i64,
        hour,
        minute,
        second,
        sub_ms,
        wd as i64,
        offset_min as i64,
    ]
}

// ---------------------------------------------------------------
// Local timezone offset — only piece that needs the OS.
// ---------------------------------------------------------------

#[cfg(unix)]
fn local_offset_min(epoch_sec: i64) -> i32 {
    // Layout matches POSIX `struct tm` with the BSD/glibc/Darwin
    // `tm_gmtoff` extension. Both Linux and macOS define it as a
    // 64-bit signed integer immediately after `tm_isdst`.
    #[repr(C)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: i64,
        tm_zone: *const std::os::raw::c_char,
    }
    unsafe extern "C" {
        fn localtime_r(time: *const i64, result: *mut Tm) -> *mut Tm;
    }
    let mut tm: MaybeUninit<Tm> = MaybeUninit::zeroed();
    unsafe {
        if localtime_r(&epoch_sec, tm.as_mut_ptr()).is_null() {
            return 0;
        }
        let tm = tm.assume_init();
        (tm.tm_gmtoff / 60) as i32
    }
}

#[cfg(windows)]
fn local_offset_min(epoch_sec: i64) -> i32 {
    #[repr(C)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
    }
    unsafe extern "C" {
        fn _localtime64_s(result: *mut Tm, time: *const i64) -> i32;
        fn _mkgmtime64(tm: *const Tm) -> i64;
    }
    let mut tm: MaybeUninit<Tm> = MaybeUninit::zeroed();
    unsafe {
        if _localtime64_s(tm.as_mut_ptr(), &epoch_sec) != 0 {
            return 0;
        }
        let tm = tm.assume_init();
        let local_as_utc = _mkgmtime64(&tm);
        ((local_as_utc - epoch_sec) / 60) as i32
    }
}

#[cfg(not(any(unix, windows)))]
fn local_offset_min(_epoch_sec: i64) -> i32 {
    0
}

// ---------------------------------------------------------------
// Clock & sleep
// ---------------------------------------------------------------

#[unsafe(export_name = "$time.now_ms")]
pub extern "C" fn time_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[unsafe(export_name = "$time.now_ns")]
pub extern "C" fn time_now_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

#[unsafe(export_name = "$time.monotonic_ns")]
pub extern "C" fn time_monotonic_ns() -> i64 {
    // Anchor the monotonic clock at the first call so subsequent
    // values are reasonably-sized i64 nanos. `Instant::now()` is
    // monotonic across the process lifetime.
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_nanos() as i64
}

#[unsafe(export_name = "$time.sleep_ms")]
pub extern "C" fn time_sleep_ms(ms: i64) {
    if ms > 0 {
        std::thread::sleep(Duration::from_millis(ms as u64));
    }
}

// ---------------------------------------------------------------
// Calendar breakdown / composition
// ---------------------------------------------------------------

#[unsafe(export_name = "$time.break_down_utc")]
pub extern "C" fn time_break_down_utc(epoch_ms: i64) -> i64 {
    let a = break_down(epoch_ms, 0);
    build_i64_array(&a, KIND_NONE)
}

#[unsafe(export_name = "$time.break_down_local")]
pub extern "C" fn time_break_down_local(epoch_ms: i64) -> i64 {
    let sec = epoch_ms.div_euclid(1000);
    let off = local_offset_min(sec);
    let a = break_down(epoch_ms, off);
    build_i64_array(&a, KIND_NONE)
}

#[unsafe(export_name = "$time.compose")]
pub extern "C" fn time_compose(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    ms: i64,
    offset_sec: i64,
) -> i64 {
    let days = days_from_civil(year as i32, month as u32, day as u32);
    let total_secs = days * 86_400 + hour * 3600 + minute * 60 + second - offset_sec;
    total_secs * 1000 + ms
}

// ---------------------------------------------------------------
// ISO 8601 / RFC 3339 parsing & formatting
// ---------------------------------------------------------------

fn read_path_str(ptr: i64) -> Option<String> {
    if ptr == 0 {
        return None;
    }
    let bytes = unsafe { cstr_bytes(ptr) };
    std::str::from_utf8(bytes).ok().map(|s| s.to_string())
}

fn pad2(buf: &mut String, n: i64) {
    if n >= 0 && n < 10 {
        buf.push('0');
    }
    buf.push_str(&n.to_string());
}

fn pad3(buf: &mut String, n: i64) {
    if n < 10 {
        buf.push_str("00");
    } else if n < 100 {
        buf.push('0');
    }
    buf.push_str(&n.to_string());
}

fn pad4(buf: &mut String, n: i64) {
    if n >= 0 {
        if n < 10 {
            buf.push_str("000");
        } else if n < 100 {
            buf.push_str("00");
        } else if n < 1000 {
            buf.push('0');
        }
        buf.push_str(&n.to_string());
    } else {
        // Negative year: prefix `-` and pad the magnitude.
        buf.push('-');
        pad4(buf, -n);
    }
}

fn build_iso(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    ms: i64,
    offset_min: i64,
) -> String {
    let mut s = String::with_capacity(32);
    pad4(&mut s, year);
    s.push('-');
    pad2(&mut s, month);
    s.push('-');
    pad2(&mut s, day);
    s.push('T');
    pad2(&mut s, hour);
    s.push(':');
    pad2(&mut s, minute);
    s.push(':');
    pad2(&mut s, second);
    if ms != 0 {
        s.push('.');
        pad3(&mut s, ms);
    }
    if offset_min == 0 {
        s.push('Z');
    } else {
        let (sign, mag) = if offset_min >= 0 {
            ('+', offset_min)
        } else {
            ('-', -offset_min)
        };
        s.push(sign);
        pad2(&mut s, mag / 60);
        s.push(':');
        pad2(&mut s, mag % 60);
    }
    s
}

#[unsafe(export_name = "$time.to_iso")]
pub extern "C" fn time_to_iso(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    ms: i64,
    offset_min: i64,
) -> i64 {
    leak_cstring(build_iso(year, month, day, hour, minute, second, ms, offset_min))
}

/// Strict-ish RFC 3339 / ISO 8601 parser.
/// Accepts: `YYYY-MM-DD` `T`|space `HH:MM:SS` `[.fff..]` `Z|±HH:MM|±HHMM`.
/// The fractional seconds field is read up to 9 digits and
/// truncated to millisecond precision. Returns the 9-slot layout
/// on success, an empty array on any rejection.
fn parse_iso(s: &str) -> Option<[i64; 9]> {
    let bytes = s.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let read_u = |off: usize, len: usize| -> Option<u64> {
        if off + len > bytes.len() {
            return None;
        }
        let mut n: u64 = 0;
        for &b in &bytes[off..off + len] {
            if !b.is_ascii_digit() {
                return None;
            }
            n = n * 10 + (b - b'0') as u64;
        }
        Some(n)
    };
    let year_sign = if bytes[0] == b'-' { -1i64 } else { 1 };
    let year_off = if year_sign < 0 { 1 } else { 0 };
    let year = read_u(year_off, 4)? as i64 * year_sign;
    if bytes[year_off + 4] != b'-' {
        return None;
    }
    let month = read_u(year_off + 5, 2)? as i64;
    if bytes[year_off + 7] != b'-' {
        return None;
    }
    let day = read_u(year_off + 8, 2)? as i64;
    let sep = bytes[year_off + 10];
    if sep != b'T' && sep != b't' && sep != b' ' {
        return None;
    }
    let hour = read_u(year_off + 11, 2)? as i64;
    if bytes[year_off + 13] != b':' {
        return None;
    }
    let minute = read_u(year_off + 14, 2)? as i64;
    if bytes[year_off + 16] != b':' {
        return None;
    }
    let second = read_u(year_off + 17, 2)? as i64;
    let mut idx = year_off + 19;
    let mut ms: i64 = 0;
    if idx < bytes.len() && bytes[idx] == b'.' {
        idx += 1;
        let frac_start = idx;
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            idx += 1;
        }
        let frac_len = idx - frac_start;
        if frac_len == 0 {
            return None;
        }
        // Convert the first 3 digits to milliseconds; ignore the rest.
        let take = frac_len.min(3);
        let mut acc: i64 = 0;
        for k in 0..take {
            acc = acc * 10 + (bytes[frac_start + k] - b'0') as i64;
        }
        for _ in take..3 {
            acc *= 10;
        }
        ms = acc;
    }
    // Offset.
    let offset_min: i64 = if idx >= bytes.len() {
        return None;
    } else {
        match bytes[idx] {
            b'Z' | b'z' => {
                idx += 1;
                0
            }
            b'+' | b'-' => {
                let sign: i64 = if bytes[idx] == b'+' { 1 } else { -1 };
                idx += 1;
                let oh = read_u(idx, 2)? as i64;
                idx += 2;
                let om = if idx < bytes.len() && bytes[idx] == b':' {
                    idx += 1;
                    let v = read_u(idx, 2)? as i64;
                    idx += 2;
                    v
                } else if idx + 2 <= bytes.len()
                    && bytes[idx].is_ascii_digit()
                    && bytes[idx + 1].is_ascii_digit()
                {
                    let v = read_u(idx, 2)? as i64;
                    idx += 2;
                    v
                } else {
                    0
                };
                sign * (oh * 60 + om)
            }
            _ => return None,
        }
    };
    if idx != bytes.len() {
        return None;
    }
    // Re-derive weekday from the date.
    let days = days_from_civil(year as i32, month as u32, day as u32);
    let weekday = weekday_from_days(days) as i64;
    Some([
        year, month, day, hour, minute, second, ms, weekday, offset_min,
    ])
}

#[unsafe(export_name = "$time.parse_iso")]
pub extern "C" fn time_parse_iso(s_ptr: i64) -> i64 {
    let Some(s) = read_path_str(s_ptr) else {
        return build_i64_array(&[], KIND_NONE);
    };
    match parse_iso(&s) {
        Some(a) => build_i64_array(&a, KIND_NONE),
        None => build_i64_array(&[], KIND_NONE),
    }
}

// ---------------------------------------------------------------
// strftime-style formatting
// ---------------------------------------------------------------

const WD_SHORT: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const WD_LONG: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MO_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MO_LONG: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn format_time(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    ms: i64,
    weekday: i64,
    offset_min: i64,
    fmt: &str,
) -> String {
    let mut out = String::with_capacity(fmt.len());
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let Some(spec) = chars.next() else {
            out.push('%');
            break;
        };
        match spec {
            'Y' => pad4(&mut out, year),
            'y' => pad2(&mut out, year.rem_euclid(100)),
            'm' => pad2(&mut out, month),
            'd' => pad2(&mut out, day),
            'H' => pad2(&mut out, hour),
            'I' => {
                let h12 = match hour % 12 {
                    0 => 12,
                    v => v,
                };
                pad2(&mut out, h12);
            }
            'M' => pad2(&mut out, minute),
            'S' => pad2(&mut out, second),
            'L' => pad3(&mut out, ms),
            'j' => {
                let doy = day_of_year(year as i32, month as u32, day as u32);
                pad3(&mut out, doy as i64);
            }
            'a' => {
                let i = ((weekday % 7) + 7) as usize % 7;
                out.push_str(WD_SHORT[i]);
            }
            'A' => {
                let i = ((weekday % 7) + 7) as usize % 7;
                out.push_str(WD_LONG[i]);
            }
            'b' => {
                let i = ((month - 1).clamp(0, 11)) as usize;
                out.push_str(MO_SHORT[i]);
            }
            'B' => {
                let i = ((month - 1).clamp(0, 11)) as usize;
                out.push_str(MO_LONG[i]);
            }
            'p' => out.push_str(if hour < 12 { "AM" } else { "PM" }),
            'z' => {
                let (sign, mag) = if offset_min >= 0 {
                    ('+', offset_min)
                } else {
                    ('-', -offset_min)
                };
                out.push(sign);
                pad2(&mut out, mag / 60);
                pad2(&mut out, mag % 60);
            }
            ':' => {
                if chars.peek() == Some(&'z') {
                    chars.next();
                    let (sign, mag) = if offset_min >= 0 {
                        ('+', offset_min)
                    } else {
                        ('-', -offset_min)
                    };
                    out.push(sign);
                    pad2(&mut out, mag / 60);
                    out.push(':');
                    pad2(&mut out, mag % 60);
                } else {
                    out.push('%');
                    out.push(':');
                }
            }
            's' => {
                // Recompose Unix seconds without ms / offset adjust:
                let days = days_from_civil(year as i32, month as u32, day as u32);
                let local = days * 86_400 + hour * 3600 + minute * 60 + second;
                let utc = local - offset_min * 60;
                out.push_str(&utc.to_string());
            }
            '%' => out.push('%'),
            'n' => out.push('\n'),
            't' => out.push('\t'),
            other => {
                // Unknown specifier — emit verbatim so users notice.
                out.push('%');
                out.push(other);
            }
        }
    }
    out
}

#[unsafe(export_name = "$time.format")]
pub extern "C" fn time_format(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    ms: i64,
    weekday: i64,
    offset_min: i64,
    fmt_ptr: i64,
) -> i64 {
    let fmt = read_path_str(fmt_ptr).unwrap_or_default();
    leak_cstring(format_time(
        year, month, day, hour, minute, second, ms, weekday, offset_min, &fmt,
    ))
}
