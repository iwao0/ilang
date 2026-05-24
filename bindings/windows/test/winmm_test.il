/// winmm — high-resolution timer + joystick count.

use std.test
use windows {
    timeGetTime, timeBeginPeriod, timeEndPeriod,
    joyGetNumDevs,
}

@extern(C) {
    pub fn run() {
        // timeGetTime is a non-zero ms count since boot.
        let t1 = timeGetTime()
        test.expectTrue(t1 > 0)

        // Raise timer resolution to 1 ms, take another sample, lower back.
        let r1 = timeBeginPeriod(1 as u32)
        test.expect(r1 as i64, 0)   // 0 = TIMERR_NOERROR
        let t2 = timeGetTime()
        test.expectTrue(t2 >= t1)
        let r2 = timeEndPeriod(1 as u32)
        test.expect(r2 as i64, 0)

        // joyGetNumDevs is a fixed value (usually 16 on modern Windows).
        let n = joyGetNumDevs()
        test.expectTrue(n > 0)
    }
}

run()
