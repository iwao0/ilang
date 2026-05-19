// shcore — process DPI awareness query (Win 8.1+).

use test
use windows {
    GetProcessDpiAwareness,
}

@extern(C) {
    pub fn run() {
        // GetProcessDpiAwareness with hProcess = NULL queries the
        // current process. Returns 0 (S_OK) and writes the mode.
        let mode: i32 = -1
        let hr = GetProcessDpiAwareness(0 as *void, &mode)
        test.expect(hr as i64, 0)
        // Mode is 0 (unaware), 1 (system aware), or 2 (per-monitor).
        test.expectTrue(mode >= 0 && mode <= 2)
    }
}

run()
