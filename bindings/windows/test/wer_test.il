/// wer — flag get / set + handle lifecycle smoke test.

use test
use windows { GetCurrentProcess }
use wer { WerSetFlags, WerGetFlags, WerFaultReporting }

@extern(C) {
    pub fn run() {
        // Save the current flags, flip noUi on, verify, restore.
        let proc = GetCurrentProcess()
        let prev: u32 = 0
        let _ = WerGetFlags(proc as i64, &prev)

        test.expect(
            WerSetFlags(WerFaultReporting.noUi as u32) as i64,
            0 as i64
        )
        let cur: u32 = 0
        test.expect(WerGetFlags(proc as i64, &cur) as i64, 0 as i64)
        test.expectTrue((cur & WerFaultReporting.noUi as u32) != 0 as u32)

        // Restore.
        let _ = WerSetFlags(prev)
    }
}

run()
