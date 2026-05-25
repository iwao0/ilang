/// user32 — clipboard format registration and Open/Close round-trip.

use std.test
use windows {
use std.ffi { cstrFromString, freeCstr, stringFromCstr }
    OpenClipboard, CloseClipboard, EmptyClipboard,
    RegisterClipboardFormatA, GetClipboardFormatNameA,
    CountClipboardFormats, IsClipboardFormatAvailable,
    GetClipboardOwner,
    HeapAlloc, HeapFree, GetProcessHeap,
    HWND,
}

@extern(C) {
    pub fn run() {
        // RegisterClipboardFormatA is callable without an open clipboard
        // and idempotent: same name always returns the same ID.
        let name = cstrFromString("Ilang_TestClipFormat")
        let f1 = RegisterClipboardFormatA(name)
        test.expectTrue(f1 >= 0xC000 as u32)
        test.expectTrue(f1 <= 0xFFFF as u32)

        let f2 = RegisterClipboardFormatA(name)
        test.expect(f1 as i64, f2 as i64)

        // Round-trip the format name.
        let heap = GetProcessHeap()
        let buf = HeapAlloc(heap, 8 as u32, 128 as u64)
        let n = GetClipboardFormatNameA(f1, buf as *char, 128 as i32)
        test.expectTrue(n > 0)
        test.expectStr(stringFromCstr(buf as *const char), "Ilang_TestClipFormat")
        HeapFree(heap, 0 as u32, buf)
        freeCstr(name)

        // Open / empty / close — exercise the lifecycle.
        test.expectTrue(OpenClipboard(0 as HWND) != 0)
        test.expectTrue(EmptyClipboard() != 0)
        // After EmptyClipboard, the owner is the calling-thread (which
        // has no HWND so this returns NULL — just verify the call).
        let _ = GetClipboardOwner()
        // CF_TEXT is format ID 1 — not currently available (we cleared
        // the clipboard).
        test.expect(IsClipboardFormatAvailable(1 as u32) as i64, 0)
        // CountClipboardFormats: 0 right now.
        test.expect(CountClipboardFormats() as i64, 0)
        test.expectTrue(CloseClipboard() != 0)
    }
}

run()
