/// msi — UI-mode setter + product-enum smoke test.

use std.test as test
use windows { GetProcessHeap, HeapAlloc, HeapFree }
use msi { MsiSetInternalUI, MsiEnumProductsA, InstallUiLevel }

@extern(C) {
    pub fn run() {
        // Save / restore the UI mode — MsiSetInternalUI returns the
        // previous value. Round-tripping should leave it unchanged.
        let prev = MsiSetInternalUI(InstallUiLevel.none as u32, 0 as *i64)
        let now = MsiSetInternalUI(InstallUiLevel.none as u32, 0 as *i64)
        test.expect(now as i64, InstallUiLevel.none as i64)
        // Restore.
        let _ = MsiSetInternalUI(prev as u32, 0 as *i64)

        // MsiEnumProductsA(0, buf) returns either ERROR_SUCCESS (0)
        // when at least one product is installed, or
        // ERROR_NO_MORE_ITEMS (259) when the database is empty.
        let heap = GetProcessHeap()
        let buf = HeapAlloc(heap, 8 as u32, 64 as u64)
        let rc = MsiEnumProductsA(0 as u32, buf as *char)
        test.expectTrue(rc == 0 as u32 || rc == 259 as u32)
        HeapFree(heap, 0 as u32, buf)
    }
}

run()
