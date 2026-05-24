// `@extern(C) struct` declares a struct whose memory layout matches a
// C struct, so it can be passed to native extern fns by pointer.
// The C side reads/writes fields directly at the user-data area;
// ilang field access uses the same offsets.
//
// `clock_gettime(CLOCK_REALTIME, &ts)` writes seconds + nanos
// into a `struct timespec`. We allocate the struct in ilang, hand
// it to libc, then read the fields back to confirm the C side
// wrote real values.
use std.test

@extern(C) {
    pub struct timespec {
        tv_sec: i64
        tv_nsec: i64
    }

    @lib("c") fn clock_gettime(clk_id: i32, tp: *timespec): i32

    /// User-level wrapper — `&ts` is only legal inside `@extern(C)`,
    /// so the struct is allocated, filled, and returned here. User
    /// code reads fields off the returned `timespec`.
    pub fn realtime(): timespec {
        let ts = new timespec()
        let _ = clock_gettime(0 as i32, &ts)
        ts
    }
}

let ts = realtime()
test.expectTrue(ts.tv_sec > 1500000000)
test.expectTrue(ts.tv_nsec >= 0)
test.expectTrue(ts.tv_nsec < 1000000000)
