/// user32 — system metrics and DPI queries.

use test
use windows {
    GetSystemMetrics, GetDpiForSystem,
    GetDesktopWindow, IsWindow, GetForegroundWindow,
    MapVirtualKeyA,
    SystemMetric, VirtualKey,
    GetCursorPos, POINT,
}

@extern(C) {
    pub fn run() {
        // Primary monitor must be at least a tiny rectangle.
        let cx = GetSystemMetrics(SystemMetric.cxScreen as i32)
        let cy = GetSystemMetrics(SystemMetric.cyScreen as i32)
        test.expectTrue(cx > 0)
        test.expectTrue(cy > 0)

        // System DPI is at least 96 (100%).
        let dpi = GetDpiForSystem()
        test.expectTrue(dpi >= 96)

        // Desktop window is a real HWND.
        let desktop = GetDesktopWindow()
        test.expectTrue((desktop as i64) != 0)
        test.expectTrue(IsWindow(desktop) != 0)

        // GetForegroundWindow may be NULL on locked screens but is a
        // sensible call.
        let fg = GetForegroundWindow()
        // Just touch the value; don't assert non-null.
        let _ = fg

        // MapVirtualKeyA: VK_A → scan code; non-zero.
        let scan = MapVirtualKeyA(VirtualKey.a as u32, 0 as u32)
        test.expectTrue(scan != 0)

        // GetCursorPos fills a POINT struct.
        let pt = POINT { x: 0, y: 0 }
        let ok = GetCursorPos(&pt)
        test.expectTrue(ok != 0)
        // Coordinates can be negative on multi-monitor setups, just
        // check the call succeeds.
    }
}

run()
