// kernel32 — heap allocation lifecycle.

use test
use windows {
    GetProcessHeap, HeapAlloc, HeapFree, HeapSize,
    GlobalAlloc, GlobalLock, GlobalUnlock, GlobalSize, GlobalFree,
    VirtualAlloc, VirtualFree,
    AllocationType, PageProtection,
}

@extern(C) {
    pub fn run() {
        let heap = GetProcessHeap()
        test.expectTrue((heap as i64) != 0)

        // HeapAlloc with zero-init (flag 0x08 = HEAP_ZERO_MEMORY).
        let p = HeapAlloc(heap, 8 as u32, 64 as u64)
        test.expectTrue((p as i64) != 0)
        test.expect(HeapSize(heap, 0 as u32, p) as i64, 64)
        test.expect(HeapFree(heap, 0 as u32, p) as i64, 1)

        // HeapAlloc without zeroing.
        let p2 = HeapAlloc(heap, 0 as u32, 128 as u64)
        test.expectTrue((p2 as i64) != 0)
        HeapFree(heap, 0 as u32, p2)

        // GlobalAlloc moveable (GMEM_MOVEABLE = 0x42).
        let h = GlobalAlloc(0x42 as u32, 32 as u64)
        test.expectTrue((h as i64) != 0)
        // Lock to access bytes.
        let locked = GlobalLock(h)
        test.expectTrue((locked as i64) != 0)
        // GlobalSize reports the actual size (≥ requested).
        test.expectTrue((GlobalSize(h) as i64) >= 32)
        GlobalUnlock(h)
        GlobalFree(h)

        // VirtualAlloc commit + release.
        let commit = AllocationType.commit | AllocationType.reserve
        let rw    = PageProtection.readWrite
        let va = VirtualAlloc(
            0 as *void,
            4096 as u64,
            commit as u32,
            rw as u32
        )
        test.expectTrue((va as i64) != 0)
        // dwSize must be 0 when freeing with MEM_RELEASE.
        let ok = VirtualFree(va, 0 as u64, AllocationType.release as u32)
        test.expectTrue(ok != 0)
    }
}

run()
