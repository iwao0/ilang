/// ole32 — COM bootstrap and task-memory allocator.

use std.test as test
use ole32 {
    CoInitializeEx, CoUninitialize,
    CoTaskMemAlloc, CoTaskMemFree, CoTaskMemRealloc,
    CoInit,
}

@extern(C) {
    pub fn run() {
        // CoInitializeEx with apartment-threaded mode. Returns 0 (S_OK)
        // or 1 (S_FALSE) if already initialised.
        let hr = CoInitializeEx(0 as *void, CoInit.apartmentThreaded as u32)
        test.expectTrue(hr == 0 || hr == 1)

        // Allocate / free task memory.
        let p = CoTaskMemAlloc(128 as u64)
        test.expectTrue((p as i64) != 0)

        // Reallocate to a larger size.
        let p2 = CoTaskMemRealloc(p, 256 as u64)
        test.expectTrue((p2 as i64) != 0)

        // Free.
        CoTaskMemFree(p2)

        // Free of NULL is a no-op.
        CoTaskMemFree(0 as *void)

        CoUninitialize()
    }
}

run()
