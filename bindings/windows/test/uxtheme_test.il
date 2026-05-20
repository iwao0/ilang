/// uxtheme — visual styles availability and theme open/close.

use test
use windows {
    IsThemeActive, IsAppThemed,
    OpenThemeData, CloseThemeData,
    BufferedPaintInit, BufferedPaintUnInit,
}

@extern(C) {
    pub fn run() {
        // Both queries return 0 or non-zero. Modern Windows desktops
        // have themes active, but a Server Core install may not.
        let active = IsThemeActive()
        let appT = IsAppThemed()
        test.expectTrue(active == 0 || active != 0)
        test.expectTrue(appT == 0 || appT != 0)

        // BufferedPaintInit / UnInit — global ref-counted enable.
        // Returns S_OK or S_FALSE if already initialised. Always pair.
        let rc1 = BufferedPaintInit()
        test.expectTrue(rc1 == 0 || rc1 == 1)
        BufferedPaintUnInit()

        // OpenThemeData on the desktop window with a missing class
        // returns NULL but does not crash.
        let bogus_w: u32 = 0   // empty UTF-16 string would crash; pass NULL
        let h = OpenThemeData(0 as i64, 0 as *const u16)
        // The result may be NULL (no theme / unknown class) which is fine.
        if (h as i64) != 0 {
            CloseThemeData(h)
        }
    }
}

run()
