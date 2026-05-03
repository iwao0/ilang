// `@repr(C) class` declares a class whose memory layout matches a
// C struct, so it can be passed to native extern fns by pointer.
// The C side reads/writes fields directly at the user-data area;
// ilang field access uses the same offsets.
//
// `clock_gettime(CLOCK_REALTIME, &ts)` writes seconds + nanos
// into a `struct timespec`. We allocate the struct in ilang, hand
// it to libc, then read the fields back to confirm the C side
// wrote real values.
// interp: skip
use test

@repr(C) class timespec {
    tv_sec: i64
    tv_nsec: i64
}

@extern("c") fn clock_gettime(clk_id: i32, tp: timespec): i32

let ts = new timespec()

// Sanity: zero-initialized.
test.expect(ts.tv_sec, 0)
test.expect(ts.tv_nsec, 0)

// CLOCK_REALTIME = 0 on macOS / Linux.
let rc = clock_gettime(0 as i32, ts)
test.expect(rc as i64, 0)

// The C side wrote into the struct. Seconds since 1970 must be
// well past 1.5 billion (year 2017 onward).
test.expectTrue(ts.tv_sec > 1500000000)
// Nanoseconds always within [0, 1_000_000_000).
test.expectTrue(ts.tv_nsec >= 0)
test.expectTrue(ts.tv_nsec < 1000000000)

// Modify a field from ilang and confirm the change is visible.
ts.tv_sec = 42
test.expect(ts.tv_sec, 42)
