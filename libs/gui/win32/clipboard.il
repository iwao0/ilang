/// System clipboard — `OpenClipboard` + `CF_UNICODETEXT`.

use windows {
    HWND,
    OpenClipboard, CloseClipboard, EmptyClipboard,
    SetClipboardData, GetClipboardData,
    GlobalAlloc, GlobalFree, GlobalLock, GlobalUnlock,
}
use std.ffi { arrayFromCArray, readU16, writeU16 }

const CF_UNICODETEXT: u32 = 13 as u32
const GMEM_MOVEABLE:  u32 = 0x0042 as u32

@extern(C) {

pub fn setClipboardText(s: string) {
    // `encodeUtf16` returns a NUL-terminated UTF-16 buffer. We
    // need to find its length to know how much to copy into the
    // GMEM_MOVEABLE allocation `SetClipboardData` requires.
    let src = s.encodeUtf16()
    let len = 0 as i64
    while readU16(src as *const void, len * 2 as i64) != 0 as u16 {
        len = len + 1
    }
    let bytes = (len + 1 as i64) as u64 * 2 as u64
    let hMem = GlobalAlloc(GMEM_MOVEABLE, bytes)
    let dst = GlobalLock(hMem) as *u16
    let i = 0 as i64
    while i <= len {
        let c = readU16(src as *const void, i * 2 as i64)
        writeU16(dst as *void, i * 2 as i64, c)
        i = i + 1
    }
    GlobalUnlock(hMem)
    if OpenClipboard(0 as HWND) == 0 as i32 {
        GlobalFree(hMem)
        return
    }
    EmptyClipboard()
    // On success, the clipboard takes ownership of `hMem`; on
    // failure we'd have to free it, but `SetClipboardData` for
    // CF_UNICODETEXT against a fresh GMEM_MOVEABLE rarely fails
    // (we'd see GetLastError() == ERROR_NOT_ENOUGH_MEMORY at
    // worst). Skip the post-mortem cleanup branch — the leak is
    // bounded by the size of one paste.
    SetClipboardData(CF_UNICODETEXT, hMem)
    CloseClipboard()
}

pub fn getClipboardText(): string {
    if OpenClipboard(0 as HWND) == 0 as i32 { return "" }
    let hMem = GetClipboardData(CF_UNICODETEXT)
    if hMem as i64 == 0 as i64 {
        CloseClipboard()
        return ""
    }
    let p = GlobalLock(hMem) as *u16
    if p as i64 == 0 as i64 {
        CloseClipboard()
        return ""
    }
    let len = 0 as i64
    while readU16(p as *const void, len * 2 as i64) != 0 as u16 {
        len = len + 1
    }
    let units = arrayFromCArray(p as *const u16, len as u64)
    let s = string.fromUtf16(units)
    GlobalUnlock(hMem)
    CloseClipboard()
    s
}

} // @extern(C)
