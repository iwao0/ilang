/// Built-in `time` module — wall clock, monotonic clock, sleep,
/// and a `DateTime` calendar type with ISO 8601 / strftime
/// parsing and formatting.
///
/// Wall-clock instants are Unix-epoch milliseconds (`i64`),
/// matching `fs.Stats.mtimeMs`. Durations are plain `i64`
/// milliseconds — no separate `Duration` class.
///
/// Timezones: only **UTC** and the **system local zone** are
/// supported. The local zone follows the host's `TZ` setting;
/// named IANA zones (e.g. `"Asia/Tokyo"`) aren't honoured.
///
/// strftime specifiers recognised by `DateTime.format`:
///
/// | spec | meaning                                                |
/// | ---- | ------------------------------------------------------ |
/// | %Y   | 4-digit year (negative years emit `-YYYY`)             |
/// | %y   | 2-digit year (`year mod 100`)                          |
/// | %m   | 2-digit month                                          |
/// | %d   | 2-digit day                                            |
/// | %H   | 2-digit hour (24h)                                     |
/// | %I   | 2-digit hour (12h)                                     |
/// | %M   | 2-digit minute                                         |
/// | %S   | 2-digit second                                         |
/// | %L   | 3-digit milliseconds                                   |
/// | %j   | 3-digit day-of-year                                    |
/// | %a   | short weekday (`Sun`..`Sat`)                           |
/// | %A   | full weekday (`Sunday`..`Saturday`)                    |
/// | %b   | short month (`Jan`..`Dec`)                             |
/// | %B   | full month (`January`..`December`)                     |
/// | %p   | `AM` / `PM`                                            |
/// | %z   | UTC offset `±HHMM`                                     |
/// | %:z  | UTC offset `±HH:MM`                                    |
/// | %s   | Unix epoch seconds                                     |
/// | %%   | literal `%`                                            |
/// | %n   | newline                                                |
/// | %t   | tab                                                    |
///
/// Unknown specifiers are echoed verbatim (`%X` → `%X`).

@intrinsic("time.now_ms")
fn time_now_ms(): i64
@intrinsic("time.now_ns")
fn time_now_ns(): i64
@intrinsic("time.monotonic_ns")
fn time_monotonic_ns(): i64
@intrinsic("time.sleep_ms")
fn time_sleep_ms(ms: i64)
@intrinsic("time.break_down_utc")
fn time_break_down_utc(epochMs: i64): i64[]
@intrinsic("time.break_down_local")
fn time_break_down_local(epochMs: i64): i64[]
@intrinsic("time.compose")
fn time_compose(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64, ms: i64, offsetSec: i64): i64
@intrinsic("time.parse_iso")
fn time_parse_iso(s: string): i64[]
@intrinsic("time.to_iso")
fn time_to_iso(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64, ms: i64, offsetMin: i64): string
@intrinsic("time.format")
fn time_format(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64, ms: i64, weekday: i64, offsetMin: i64, fmt: string): string
@intrinsic("time.set_timeout")
fn time_set_timeout(ms: i64, callback: fn()): i64
@intrinsic("time.clear_timeout")
fn time_clear_timeout(id: i64)
@intrinsic("time.set_interval")
fn time_set_interval(ms: i64, callback: fn()): i64
@intrinsic("time.clear_interval")
fn time_clear_interval(id: i64)

/// Current wall-clock instant as Unix-epoch milliseconds.
pub fn now(): i64 { time_now_ms() }

/// Current wall-clock instant as Unix-epoch seconds.
pub fn nowSec(): i64 { time_now_ms() / 1000 }

/// Current wall-clock instant as Unix-epoch nanoseconds.
/// Precision is platform-dependent (typically µs on macOS, ns on
/// Linux).
pub fn nowNanos(): i64 { time_now_ns() }

/// Monotonic nanoseconds since process start. Use for measuring
/// elapsed durations — guaranteed not to go backwards when the
/// wall clock is adjusted (NTP slew, DST, manual change).
pub fn monotonicNs(): i64 { time_monotonic_ns() }

/// Block the current thread for `ms` milliseconds. No-op for
/// `ms <= 0`. The actual sleep length is at least `ms` but the
/// OS may wake later — don't use this for sub-millisecond timing.
pub fn sleep(ms: i64) { time_sleep_ms(ms) }

/// Schedule `callback` to run once after `ms` milliseconds. Returns
/// a non-zero id; pass it to `clearTimeout` to cancel before the
/// timer fires. Callbacks run on the shared pool worker threads —
/// the runtime drains pending timers at the program-end so a
/// scheduled `setTimeout` reliably fires before the process exits.
/// Argument order mirrors JavaScript's `setTimeout(fn, ms)`.
pub fn setTimeout(callback: fn(), ms: i64): i64 {
    time_set_timeout(ms, callback)
}

/// Cancel a pending `setTimeout`. No-op for unknown or already-
/// fired ids.
pub fn clearTimeout(id: i64) { time_clear_timeout(id) }

/// Schedule `callback` to run every `ms` milliseconds until
/// `clearInterval(id)` cancels it. The first firing is `ms` after
/// the call (not immediate). An uncleared interval keeps the
/// process alive — matches Node.js semantics.
pub fn setInterval(callback: fn(), ms: i64): i64 {
    time_set_interval(ms, callback)
}

/// Cancel a `setInterval` schedule. Same id space as
/// `clearTimeout`, but the name is kept distinct for parity with
/// JavaScript's API.
pub fn clearInterval(id: i64) { time_clear_interval(id) }

/// Broken-down calendar time at a specific UTC offset. Stores
/// wall-clock components plus the offset they're interpreted
/// under — converting back to a Unix instant honours the stored
/// offset.
///
/// Construct via `DateTime.now()` / `DateTime.utcNow()` /
/// `DateTime.fromUnixMs(...)` / `DateTime.parseIso(...)`; the
/// `init` constructor exists mainly for the static factories.
pub class DateTime {
    /// Proleptic-Gregorian year. Negative for BCE (year 0 = 1 BCE).
    pub year: i32
    /// Month, 1..12.
    pub month: i32
    /// Day of month, 1..31.
    pub day: i32
    /// Hour, 0..23.
    pub hour: i32
    /// Minute, 0..59.
    pub minute: i32
    /// Second, 0..59 (leap seconds are clamped — POSIX time has no
    /// 60).
    pub second: i32
    /// Sub-second remainder in milliseconds, 0..999.
    pub ms: i32
    /// Day of week: 0=Sun, 1=Mon, ..., 6=Sat.
    pub weekday: i32
    /// Minutes east of UTC. `0` for a `DateTime` in UTC, the
    /// host's offset for one constructed via `now()` /
    /// `fromUnixMsLocal()`.
    pub offsetMin: i32

    pub init(year: i32, month: i32, day: i32, hour: i32, minute: i32, second: i32, ms: i32, weekday: i32, offsetMin: i32) {
        this.year = year
        this.month = month
        this.day = day
        this.hour = hour
        this.minute = minute
        this.second = second
        this.ms = ms
        this.weekday = weekday
        this.offsetMin = offsetMin
    }

    /// Convert Unix-epoch milliseconds to a `DateTime` in UTC.
    pub static fromUnixMs(epochMs: i64): DateTime {
        fromArr(time_break_down_utc(epochMs))
    }

    /// Convert Unix-epoch milliseconds to a `DateTime` in the
    /// host's local timezone.
    pub static fromUnixMsLocal(epochMs: i64): DateTime {
        fromArr(time_break_down_local(epochMs))
    }

    /// Current wall time in UTC.
    pub static utcNow(): DateTime {
        DateTime.fromUnixMs(time_now_ms())
    }

    /// Current wall time in the host's local timezone.
    pub static now(): DateTime {
        DateTime.fromUnixMsLocal(time_now_ms())
    }

    /// Parse an ISO 8601 / RFC 3339 timestamp.
    ///
    /// Accepted shapes:
    /// `YYYY-MM-DDTHH:MM:SS[.fff][Z|±HH:MM|±HHMM]`
    ///
    /// The date / time separator can be `T`, `t`, or a single
    /// space. Fractional seconds beyond 3 digits are silently
    /// truncated. A `Z` (or `z`) suffix denotes UTC; a numeric
    /// offset stores the offset on the returned `DateTime`.
    /// Returns `Result.err("...")` for anything outside this
    /// grammar.
    pub static parseIso(s: string): Result<DateTime, string> {
        let arr = time_parse_iso(s)
        if arr.length == 0 {
            return Result.err("invalid ISO 8601 timestamp")
        }
        Result.ok(fromArr(arr))
    }

    /// Convert back to Unix-epoch milliseconds. Honours the stored
    /// `offsetMin` — `DateTime.fromUnixMs(t).toUnixMs() == t` for
    /// any `t`.
    pub toUnixMs(): i64 {
        time_compose(
            this.year as i64, this.month as i64, this.day as i64,
            this.hour as i64, this.minute as i64, this.second as i64,
            this.ms as i64, (this.offsetMin as i64) * 60
        )
    }

    /// Format as ISO 8601 / RFC 3339. Emits the stored offset
    /// (`Z` when it is zero). Fractional seconds are omitted when
    /// `ms == 0`.
    pub toIso(): string {
        time_to_iso(
            this.year as i64, this.month as i64, this.day as i64,
            this.hour as i64, this.minute as i64, this.second as i64,
            this.ms as i64, this.offsetMin as i64
        )
    }

    /// strftime-style format. See the module-level doc table for
    /// the recognised specifiers.
    pub format(fmt: string): string {
        time_format(
            this.year as i64, this.month as i64, this.day as i64,
            this.hour as i64, this.minute as i64, this.second as i64,
            this.ms as i64, this.weekday as i64, this.offsetMin as i64,
            fmt
        )
    }
}

fn fromArr(arr: i64[]): DateTime {
    new DateTime(
        arr[0] as i32, arr[1] as i32, arr[2] as i32,
        arr[3] as i32, arr[4] as i32, arr[5] as i32,
        arr[6] as i32, arr[7] as i32, arr[8] as i32
    )
}
