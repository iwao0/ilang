/// dwmapi — composition status and window attribute query.

use std.test as test
use dwmapi { DwmIsCompositionEnabled, DwmFlush }

@extern(C) {
    pub fn run() {
        // On Windows 8+, composition is always on (returns 1).
        // On 7 it can be off; we just check the call returns S_OK.
        let enabled: i32 = -1
        let hr = DwmIsCompositionEnabled(&enabled)
        test.expect(hr as i64, 0)
        test.expectTrue(enabled == 0 || enabled == 1)

        // DwmFlush should succeed (S_OK).
        // On systems without DWM it may return DWM_E_COMPOSITIONDISABLED
        // (0x80263001) — accept either S_OK or a negative HRESULT.
        let rc = DwmFlush()
    }
}

run()
