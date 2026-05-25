/// comctl32 — basic init + image list lifecycle.

use std.test as test
use windows {
    INITCOMMONCONTROLSEX, InitCommonControlsEx, Icc,
    ImageList_Create, ImageList_Destroy, ImageList_GetImageCount,
    ImageListFlags,
}

@extern(C) {
    pub fn run() {
        // Bootstrap common controls v6. Without an app manifest
        // the call may report failure even though the v5 classes
        // are registered; just verify the call links and returns.
        let icc = INITCOMMONCONTROLSEX {
            dwSize: 8 as u32,
            dwICC:  Icc.standardClasses as u32 | Icc.barClasses as u32,
        }
        let _ = InitCommonControlsEx(&icc)

        // Image list: create empty 16x16 → destroy.
        let himl = ImageList_Create(
            16, 16,
            ImageListFlags.color32 as u32 | ImageListFlags.mask as u32,
            0, 1
        )
        test.expectTrue((himl as i64) != 0)
        test.expect(ImageList_GetImageCount(himl) as i64, 0 as i64)
        test.expectTrue(ImageList_Destroy(himl) != 0)
    }
}

run()
