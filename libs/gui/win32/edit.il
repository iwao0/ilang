/// Single-line text input — EDIT window class.
///
/// EN_CHANGE notifications come in via `wnd_proc`'s WM_COMMAND
/// arm; we expose `tryEditChange(hwnd)` so the proc doesn't have
/// to reach into our `EDITS` array directly.

use windows {
    HWND, HINSTANCE, HMENU,
    WindowStyle, ShowCmd,
    GetModuleHandleA,
    CreateWindowExW, SetWindowTextW, EnableWindow, MoveWindow, ShowWindow,
    GetWindowTextW, GetWindowTextLengthW,
    HeapAlloc, HeapFree, GetProcessHeap,
    WideCharToMultiByte,
}
use util { toWideUtf8, applyGuiFont, noOpText }
use core { windowHwnd }

class EditSlot {
    pub hwnd:     HWND
    pub onChange: fn(string)
    pub init(hwnd: HWND) {
        this.hwnd = hwnd
        this.onChange = noOpText
    }
}

let EDITS: EditSlot[] = []

fn findEditSlot(hwnd: i64): i64 {
    let n = EDITS.length
    let i: i64 = 0
    let found: i64 = -1
    while i < n {
        if EDITS[i].hwnd as i64 == hwnd {
            found = i
            i = n
        } else {
            i = i + 1
        }
    }
    found
}

/// Called by `wnd_proc` for WM_COMMAND / EN_CHANGE.
pub fn tryEditChange(hwnd: i64): bool {
    let ix = findEditSlot(hwnd)
    if ix < 0 { return false }
    let cb = EDITS[ix].onChange
    cb(readEditText(ix))
    true
}

@extern(C) {

pub fn createNativeEdit(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let id: i64 = EDITS.length as i64
    let classW = toWideUtf8("EDIT") as *u16
    let textW  = toWideUtf8(text) as *u16
    let visibleBits: u32 = if visible {
        WindowStyle.visible as u32
    } else {
        0 as u32
    }
    // WS_BORDER + ES_AUTOHSCROLL (0x80) — input-looking and
    // scrolls horizontally past the right edge.
    let style: u32 = (WindowStyle.child as u32)
                   | (WindowStyle.border as u32)
                   | visibleBits
                   | (0x0080 as u32)
    let hInstance = GetModuleHandleA(0 as *const char)
    let hwnd = CreateWindowExW(
        0,
        classW,
        textW,
        style,
        x as i32, y as i32, width as i32, height as i32,
        parent,
        id as HMENU,
        hInstance as HINSTANCE,
        0 as *void
    )
    HeapFree(GetProcessHeap(), 0 as u32, classW as *void)
    HeapFree(GetProcessHeap(), 0 as u32, textW as *void)
    let enabledFlag: i32 = if enabled { 1 } else { 0 }
    EnableWindow(hwnd, enabledFlag)
    applyGuiFont(hwnd as i64)
    EDITS.push(new EditSlot(hwnd))
    id
}

/// Read the current contents of edit `handle` as a UTF-8 string.
pub fn readEditText(handle: i64): string {
    if handle < 0 || handle >= (EDITS.length as i64) { return "" }
    let hwnd = EDITS[handle].hwnd
    let lenW = GetWindowTextLengthW(hwnd) as i32
    if lenW <= 0 { return "" }
    let heap = GetProcessHeap()
    let wideBuf = HeapAlloc(heap, 8 as u32, (lenW as u64 + 1 as u64) * 2 as u64) as *u16
    let _ = GetWindowTextW(hwnd, wideBuf, lenW + 1)
    let cap: i32 = (lenW + 1) * 3
    let utf8Buf = HeapAlloc(heap, 8 as u32, cap as u64) as *char
    let _ = WideCharToMultiByte(
        65001 as u32,           // CP_UTF8
        0 as u32,
        wideBuf as *const u16,
        -1 as i32,
        utf8Buf,
        cap,
        0 as *const char,
        0 as *i32
    )
    let s = stringFromCstr(utf8Buf as *const char)
    HeapFree(heap, 0 as u32, wideBuf as *void)
    HeapFree(heap, 0 as u32, utf8Buf as *void)
    s
}

pub fn setOnEditChange(handle: i64, cb: fn(string)) {
    if handle >= 0 && handle < (EDITS.length as i64) {
        EDITS[handle].onChange = cb
    }
}
pub fn setEditText(handle: i64, text: string) {
    if handle >= 0 && handle < (EDITS.length as i64) {
        let w = toWideUtf8(text) as *u16
        let _ = SetWindowTextW(EDITS[handle].hwnd, w)
        HeapFree(GetProcessHeap(), 0 as u32, w as *void)
    }
}
pub fn setEditFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (EDITS.length as i64) {
        MoveWindow(EDITS[handle].hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setEditEnabled(handle: i64, enabled: bool) {
    if handle >= 0 && handle < (EDITS.length as i64) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(EDITS[handle].hwnd, flag)
    }
}
pub fn setEditVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (EDITS.length as i64) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(EDITS[handle].hwnd, cmd)
    }
}

} // @extern(C)
