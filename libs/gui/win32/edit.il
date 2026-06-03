/// Single-line text input — EDIT window class.
///
/// EN_CHANGE notifications come in via `wnd_proc`'s WM_COMMAND
/// arm; we expose `tryEditChange(hwnd)` so the proc doesn't have
/// to reach into our `EDITS` map directly.

use windows {
    HWND, HINSTANCE, HMENU, RECT,
    WindowStyle, ShowCmd,
    TOOLINFOW, TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    GetModuleHandleA,
    CreateWindowExW, SetWindowTextW, EnableWindow, MoveWindow, ShowWindow,
    SendMessageW,
    GetWindowTextW, GetWindowTextLengthW,
    HeapAlloc, HeapFree, GetProcessHeap,
}
use std.ffi { arrayFromCArray }
use util { applyGuiFont, noOpText }
use core { windowHwnd }

class EditSlot {
    pub hwnd:     HWND
    pub onChange: fn(string)
    pub winHandle: i64
    /// Lazy-created tooltip control; stays `0` until the first
    /// non-empty `setEditTooltip` call. Mirrors the Button-side
    /// pattern.
    pub tooltipHwnd: HWND
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd = hwnd
        this.onChange = noOpText
        this.winHandle = winHandle
        this.tooltipHwnd = 0 as HWND
    }
}

let EDITS = new Map<i64, EditSlot>()

/// Called by `wnd_proc` for WM_COMMAND / EN_CHANGE.
pub fn tryEditChange(hwnd: i64): bool {
    if let some(slot) = EDITS.get(hwnd) {
        slot.onChange(readEditText(hwnd))
        true
    } else {
        false
    }
}

@extern(C) {

pub fn createNativeEdit(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = "EDIT".encodeUtf16()
    let textW  = text.encodeUtf16()
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
        0 as HMENU,
        hInstance as HINSTANCE,
        0 as *void
    )
    let enabledFlag: i32 = if enabled { 1 } else { 0 }
    EnableWindow(hwnd, enabledFlag)
    applyGuiFont(hwnd)
    let handle = hwnd as i64
    EDITS[handle] = new EditSlot(hwnd, windowHandle)
    handle
}

/// Read the current contents of edit `handle` as a UTF-8 string.
pub fn readEditText(handle: i64): string {
    if let some(slot) = EDITS.get(handle) {
        let hwnd = slot.hwnd
        let lenW = GetWindowTextLengthW(hwnd) as i32
        if lenW <= 0 {
            ""
        } else {
            // GetWindowTextW writes into a sized C buffer, so we
            // still need `HeapAlloc` for the write target. Convert
            // the result into a `u16[]` (just `lenW` units, no
            // trailing NUL) via `arrayFromCArray`, then decode via
            // `string.fromUtf16`.
            let heap = GetProcessHeap()
            let wideBuf = HeapAlloc(heap, 8 as u32, (lenW as u64 + 1 as u64) * 2 as u64) as *u16
            GetWindowTextW(hwnd, wideBuf, lenW + 1)
            let units = arrayFromCArray(wideBuf as *const u16, lenW as u64)
            HeapFree(heap, 0 as u32, wideBuf as *void)
            string.fromUtf16(units)
        }
    } else {
        ""
    }
}

pub fn setOnEditChange(handle: i64, cb: fn(string)) {
    if let some(slot) = EDITS.get(handle) { slot.onChange = cb }
}
pub fn setEditText(handle: i64, text: string) {
    if let some(slot) = EDITS.get(handle) {
        SetWindowTextW(slot.hwnd, text.encodeUtf16())
    }
}
pub fn setEditFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = EDITS.get(handle) {
        MoveWindow(slot.hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setEditEnabled(handle: i64, enabled: bool) {
    if let some(slot) = EDITS.get(handle) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
    }
}
pub fn setEditVisible(handle: i64, visible: bool) {
    if let some(slot) = EDITS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}
pub fn setEditTooltip(handle: i64, text: string) {
    if let some(slot) = EDITS.get(handle) {
        // Empty string → drop the tool (if any). Same logic the
        // Button backend uses; see win32/button.il for the full
        // notes on TOOLINFOW layout and TTF_SUBCLASS coexistence.
        if text == "" {
            if slot.tooltipHwnd as i64 != 0 as i64 {
                let info = TOOLINFOW {
                    cbSize:     72 as u32,
                    uFlags:     TTF_IDISHWND,
                    hwnd:       slot.winHandle as HWND,
                    uId:        slot.hwnd as u64,
                    rect:       RECT { left: 0 as i32, top: 0 as i32, right: 0 as i32, bottom: 0 as i32 },
                    hinst:      0 as HINSTANCE,
                    lpszText:   0 as *const u16,
                    lParam:     0 as i64,
                    lpReserved: 0 as *void,
                }
                let p: *const TOOLINFOW = &info
                SendMessageW(slot.tooltipHwnd, TTM_DELTOOLW, 0 as u64, p as i64)
            }
            return
        }
        let textW = text.encodeUtf16()
        let info = TOOLINFOW {
            cbSize:     72 as u32,
            uFlags:     TTF_IDISHWND | TTF_SUBCLASS,
            hwnd:       slot.winHandle as HWND,
            uId:        slot.hwnd as u64,
            rect:       RECT { left: 0 as i32, top: 0 as i32, right: 0 as i32, bottom: 0 as i32 },
            hinst:      0 as HINSTANCE,
            lpszText:   textW,
            lParam:     0 as i64,
            lpReserved: 0 as *void,
        }
        let p: *const TOOLINFOW = &info
        if slot.tooltipHwnd as i64 == 0 as i64 {
            let classW = "tooltips_class32".encodeUtf16()
            let hInstance = GetModuleHandleA(0 as *const char)
            let tt = CreateWindowExW(
                0,
                classW,
                0 as *const u16,
                (WindowStyle.popup as u32) | TTS_ALWAYSTIP,
                0, 0, 0, 0,
                slot.winHandle as HWND,
                0 as HMENU,
                hInstance as HINSTANCE,
                0 as *void
            )
            slot.tooltipHwnd = tt
            SendMessageW(tt, TTM_ADDTOOLW, 0 as u64, p as i64)
        } else {
            SendMessageW(slot.tooltipHwnd, TTM_UPDATETIPTEXTW, 0 as u64, p as i64)
        }
    }
}

} // @extern(C)
