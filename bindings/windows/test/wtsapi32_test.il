/// wtsapi32 — session information for the calling process.

use std.test as test
use wtsapi32 {
    WTSGetActiveConsoleSessionId,
    WTSEnumerateSessionsA,
    WTSFreeMemory,
    WTS_SESSION_INFO,
}

@extern(C) {
    pub fn run() {
        // Active console session — always a valid u32 except when no
        // user is logged in (in which case it is 0xFFFFFFFF).
        let sid = WTSGetActiveConsoleSessionId()

        // Enumerate sessions on the local server.
        let info: *WTS_SESSION_INFO = 0 as *WTS_SESSION_INFO
        let count: u32 = 0
        let ok = WTSEnumerateSessionsA(
            0 as *void,        // local server
            0 as u32,           // Reserved
            1 as u32,           // Version
            &info,
            &count
        )
        test.expectTrue(ok != 0)
        test.expectTrue(count > 0)
        test.expectTrue((info as i64) != 0)

        WTSFreeMemory(info as *void)
    }
}

run()
