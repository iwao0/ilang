/// winsta — open server + enumerate sessions smoke test.

use test
use winsta {
    WinStationOpenServerW, WinStationCloseServer,
    WinStationEnumerateW, WinStationFreeMemory,
    SESSIONIDW,
}

@extern(C) {
    pub fn run() {
        // Open local server. Returns a non-zero pseudo-handle (sentinel
        // for "local") on success.
        let h = WinStationOpenServerW(0 as *const u16)
        test.expectTrue(h != 0 as i64)

        let pInfo: *SESSIONIDW = 0 as *SESSIONIDW
        let count: u32 = 0
        // Every live system has at least one session (the SYSTEM /
        // console). Non-zero return = success.
        test.expectTrue(WinStationEnumerateW(h, &pInfo, &count) != 0)
        test.expectTrue(count > 0 as u32)

        WinStationFreeMemory(pInfo as *void)
        WinStationCloseServer(h)
    }
}

run()
