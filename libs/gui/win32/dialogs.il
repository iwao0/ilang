/// Modal dialogs — `MessageBoxW` for alert / confirm, comdlg32's
/// `GetOpenFileNameW` / `GetSaveFileNameW` for the file pickers.
///
/// All three Win32 APIs are synchronous: they spin a nested
/// message loop and return when the user dismisses the dialog.
/// The public-facing callback is invoked just before the wrapper
/// function returns, so the API shape matches the cocoa / linux
/// sheet-modal flow (`window.alert(text, fn() {...})`) even
/// though Win32 doesn't have a sheet primitive.

use windows {
    HWND, HINSTANCE,
    MessageBoxType, MessageBoxResult,
    MessageBoxW,
    OPENFILENAMEA, GetOpenFileNameW, GetSaveFileNameW,
    HANDLE, GetProcessHeap, HeapAlloc, HeapFree,
}
use std.ffi { arrayFromCArray, readU16 }
use core { windowHwnd }

@extern(C) {

/// Walk a null-terminated UTF-16 buffer to recover its length in
/// `u16` units, then convert back to an ilang string.
fn utf16BufToString(buf: *u16): string {
    let len = 0 as i64
    while readU16(buf as *const void, len * 2 as i64) != 0 as u16 {
        len = len + 1
    }
    let units = arrayFromCArray(buf as *const u16, len as u64)
    string.fromUtf16(units)
}

pub fn showAlertDialog(windowHandle: i64, text: string, callback: fn()) {
    let hwnd = windowHwnd(windowHandle)
    let textW = text.encodeUtf16()
    let titleW = "".encodeUtf16()
    let flags = (MessageBoxType.ok as u32)
              | (MessageBoxType.iconInformation as u32)
    MessageBoxW(hwnd, textW, titleW, flags)
    callback()
}

pub fn showConfirmDialog(windowHandle: i64, text: string, callback: fn(bool)) {
    let hwnd = windowHwnd(windowHandle)
    let textW = text.encodeUtf16()
    let titleW = "".encodeUtf16()
    let flags = (MessageBoxType.okCancel as u32)
              | (MessageBoxType.iconQuestion as u32)
    let r = MessageBoxW(hwnd, textW, titleW, flags)
    callback(r == MessageBoxResult.ok as i32)
}

/// `MAX_PATH` slack including trailing NUL. The win32 dialog
/// writes the chosen path here; 1024 covers the long-path case
/// callers might enable.
const PATH_BUF_LEN: i64 = 1024

fn allocPathBuffer(): *u16 {
    let heap = GetProcessHeap()
    // HEAP_ZERO_MEMORY = 0x08 so the buffer starts NUL — saves
    // a manual write before the dialog runs in case the user
    // cancels before typing anything.
    HeapAlloc(heap, 0x08 as u32, (PATH_BUF_LEN as u64) * 2 as u64) as *u16
}

fn freePathBuffer(buf: *u16) {
    HeapFree(GetProcessHeap(), 0 as u32, buf as *void)
}

fn fillOpenFileName(hwnd: HWND, buf: *u16): OPENFILENAMEA {
    OPENFILENAMEA {
        lStructSize:       88 as u32,
        hwndOwner:         hwnd,
        hInstance:         0 as HINSTANCE,
        lpstrFilter:       0 as *const char,
        lpstrCustomFilter: 0 as *char,
        nMaxCustFilter:    0 as u32,
        nFilterIndex:      0 as u32,
        lpstrFile:         buf as *char,
        nMaxFile:          PATH_BUF_LEN as u32,
        lpstrFileTitle:    0 as *char,
        nMaxFileTitle:     0 as u32,
        lpstrInitialDir:   0 as *const char,
        lpstrTitle:        0 as *const char,
        // OFN_EXPLORER (0x80000) | OFN_HIDEREADONLY (0x4)
        // — modern look + drop the redundant read-only checkbox.
        Flags:             0x80004 as u32,
        nFileOffset:       0 as u16,
        nFileExtension:    0 as u16,
        lpstrDefExt:       0 as *const char,
        lCustData:         0 as i64,
        lpfnHook:          0 as *void,
        lpTemplateName:    0 as *const char,
        pvReserved:        0 as *void,
        dwReserved:        0 as u32,
        FlagsEx:           0 as u32,
    }
}

pub fn showOpenFileDialog(windowHandle: i64, callback: fn(string)) {
    let hwnd = windowHwnd(windowHandle)
    let buf = allocPathBuffer()
    let ofn = fillOpenFileName(hwnd, buf)
    let p: *OPENFILENAMEA = &ofn
    if GetOpenFileNameW(p) == 0 as i32 {
        freePathBuffer(buf)
        callback("")
        return
    }
    let s = utf16BufToString(buf)
    freePathBuffer(buf)
    callback(s)
}

pub fn showSaveFileDialog(
    windowHandle: i64, suggestedName: string, callback: fn(string)
) {
    let hwnd = windowHwnd(windowHandle)
    let buf = allocPathBuffer()
    // Pre-seed the buffer with the suggested name so the dialog
    // opens with that text already in the filename field.
    if suggestedName != "" {
        let seedW = suggestedName.encodeUtf16()
        // Copy until the source NUL or until we run out of room.
        let i = 0 as i64
        while i < PATH_BUF_LEN - 1 as i64 {
            let c = readU16(seedW as *const void, i * 2 as i64)
            if c == 0 as u16 { break }
            // 2 bytes per UTF-16 unit; `*(buf + i*2)` write.
            (buf as *u16 + i as i64)[0] = c
            i = i + 1
        }
    }
    let ofn = fillOpenFileName(hwnd, buf)
    let p: *OPENFILENAMEA = &ofn
    if GetSaveFileNameW(p) == 0 as i32 {
        freePathBuffer(buf)
        callback("")
        return
    }
    let s = utf16BufToString(buf)
    freePathBuffer(buf)
    callback(s)
}

} // @extern(C)
