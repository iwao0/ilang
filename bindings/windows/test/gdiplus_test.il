/// gdiplus — startup / shutdown lifecycle.

use std.test as test
use gdiplus { GdiplusStartup, GdiplusShutdown, GdiplusStartupInput }

@extern(C) {
    pub fn run() {
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: 0 as *void,
            SuppressBackgroundThread: 0,
            SuppressExternalCodecs: 0,
        }
        let token: u64 = 0
        let rc = GdiplusStartup(&token, &input, 0 as *void)
        // 0 = Ok.
        test.expect(rc as i64, 0)
        test.expectTrue(token != 0 as u64)

        GdiplusShutdown(token)
    }
}

run()
