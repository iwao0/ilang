/// user32 — window introspection on the desktop HWND.

use std.test
use windows {
    HWND, RECT, POINT,
    GetDesktopWindow, GetForegroundWindow,
    IsWindow, IsWindowVisible,
    GetWindowRect, GetClientRect,
    GetWindowLongA, GetWindowLongPtrA, WindowLongIndex,
    GetWindowTextLengthA,
    GetWindowThreadProcessId,
    ScreenToClient,
    FindWindowA,
}

@extern(C) {
    pub fn run() {
        let desktop = GetDesktopWindow()
        test.expectTrue((desktop as i64) != 0)
        test.expectTrue(IsWindow(desktop) != 0)
        test.expectTrue(IsWindowVisible(desktop) != 0)

        // GetWindowRect must fill a non-degenerate RECT.
        let wr = RECT { left: 0, top: 0, right: 0, bottom: 0 }
        test.expectTrue(GetWindowRect(desktop, &wr) != 0)
        test.expectTrue(wr.right > wr.left)
        test.expectTrue(wr.bottom > wr.top)

        let cr = RECT { left: 0, top: 0, right: 0, bottom: 0 }
        test.expectTrue(GetClientRect(desktop, &cr) != 0)
        test.expectTrue(cr.right >= 0)

        // Style bit for the desktop includes WS_VISIBLE (0x10000000).
        let style = GetWindowLongA(desktop, WindowLongIndex.style as i32)
        test.expectTrue((style & 0x10000000) != 0)

        // GetWindowLongPtrA returns an i64-sized value with the same
        // visibility bit set.
        let style2 = GetWindowLongPtrA(desktop, WindowLongIndex.style as i32)
        test.expectTrue((style2 & 0x10000000) != 0)

        // Desktop has no caption text — length is 0.
        test.expect(GetWindowTextLengthA(desktop) as i64, 0)

        // FindWindowA for a sentinel class returns NULL (no crash).
        let bogus = cstrFromString("__no_such_class__")
        test.expect(FindWindowA(bogus, 0 as *const char) as i64, 0)
        freeCstr(bogus)

        // GetWindowThreadProcessId yields a non-zero PID for the desktop.
        let pid: u32 = 0
        test.expectTrue(GetWindowThreadProcessId(desktop, &pid) != 0)
        test.expectTrue(pid != 0)

        // ScreenToClient transforms a fixed screen point.
        let pt = POINT { x: 100, y: 100 }
        test.expectTrue(ScreenToClient(desktop, &pt) != 0)
        let _ = pt   // touch
    }
}

run()
