/// Single-line text input — EDIT window class.
///
/// EN_CHANGE notifications come in via `wnd_proc`'s WM_COMMAND
/// arm; we expose `tryEditChange(hwnd)` so the proc doesn't have
/// to reach into our `EDITS` map directly.

use windows {
    HWND, HINSTANCE, HMENU, RECT,
    WindowStyle, ShowCmd, WindowMessage, TrackMouseFlags, TRACKMOUSEEVENT,
    VirtualKey,
    TOOLINFOW, TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    GetModuleHandleA,
    CreateWindowExW, SetWindowTextW, EnableWindow, MoveWindow, ShowWindow,
    SendMessageW, SetFocus, GetFocus, TrackMouseEvent,
    SetWindowSubclass, DefSubclassProc,
    GetWindowTextW, GetWindowTextLengthW,
    HeapAlloc, HeapFree, GetProcessHeap,
}
use std.ffi { arrayFromCArray }
use util {
    applyGuiFont, noOpText,
    vkToString, modCtrl, modShift, modAlt, modMeta,
}
use core { windowHwnd, noOp1, noOpEnterLeave, noOpClickMod, noOpWheel, noOpKey }

/// EDIT-control messages. Constants from `winuser.h`.
/// EM_GETSEL      — fill (start, end) via output u32 pointers.
/// EM_SETSEL      — set selection to wParam..lParam (-1 = end).
/// EM_LIMITTEXT   — cap on user-typed character count.
/// EM_SETREADONLY — wParam = TRUE → block typed edits, retain
///                  copy / selection.
/// EM_SETCUEBANNER — cue-banner (placeholder). wParam = retain
///                   when focused?, lParam = wide-char pointer.
const EM_GETSEL:        u32 = 0x00B0 as u32
const EM_SETSEL:        u32 = 0x00B1 as u32
const EM_LIMITTEXT:     u32 = 0x00C5 as u32
const EM_SETREADONLY:   u32 = 0x00CF as u32
const EM_SETCUEBANNER:  u32 = 0x1501 as u32

class EditSlot {
    pub hwnd:     HWND
    pub onChange: fn(string)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onKeyDown:    fn(string, bool, bool, bool, bool)
    pub onKeyUp:      fn(string, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub mouseTracking: bool
    pub winHandle: i64
    /// Lazy-created tooltip control; stays `0` until the first
    /// non-empty `setEditTooltip` call. Mirrors the Button-side
    /// pattern.
    pub tooltipHwnd: HWND
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd = hwnd
        this.onChange = noOpText
        this.onFocus  = noOp1
        this.onBlur   = noOp1
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseDown  = noOpClickMod
        this.onMouseUp    = noOpClickMod
        this.onContextMenu = noOpClickMod
        this.onKeyDown     = noOpKey
        this.onKeyUp       = noOpKey
        this.onMouseWheel  = noOpWheel
        this.mouseTracking = false
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
/// Called by `wnd_proc` for WM_COMMAND / EN_SETFOCUS.
pub fn tryEditFocus(hwnd: i64): bool {
    if let some(slot) = EDITS.get(hwnd) {
        let cb = slot.onFocus
        cb()
        true
    } else {
        false
    }
}
/// Called by `wnd_proc` for WM_COMMAND / EN_KILLFOCUS.
pub fn tryEditBlur(hwnd: i64): bool {
    if let some(slot) = EDITS.get(hwnd) {
        let cb = slot.onBlur
        cb()
        true
    } else {
        false
    }
}

@extern(C) {

/// Window-procedure subclass installed on every EDIT instance.
/// Surfaces the per-control mouse events the default EDIT
/// wndproc would otherwise consume on its own. Forwards
/// everything else through `DefSubclassProc` so caret handling,
/// drag-select, etc. stay intact.
pub fn editSubclassProc(
    hwnd: i64, msg: u32, wParam: u64, lParam: i64,
    idSubclass: u64, refData: u64
): i64 {
    let lo = (lParam & 0xFFFF) as f64
    let hi = ((lParam >> 16) & 0xFFFF) as f64
    match msg as WindowMessage {
        WindowMessage.mouseMove {
            if let some(slot) = EDITS.get(hwnd) {
                if !slot.mouseTracking {
                    let tme = TRACKMOUSEEVENT {
                        cbSize:      24 as u32,
                        dwFlags:     TrackMouseFlags.leave as u32,
                        hwndTrack:   hwnd as HWND,
                        dwHoverTime: 0 as u32,
                    }
                    TrackMouseEvent(&tme)
                    slot.mouseTracking = true
                    let cb = slot.onMouseEnter
                    cb(lo, hi)
                }
            }
        }
        WindowMessage.mouseLeave {
            if let some(slot) = EDITS.get(hwnd) {
                slot.mouseTracking = false
                let cb = slot.onMouseLeave
                cb(lo, hi)
            }
        }
        WindowMessage.lbuttondown {
            if let some(slot) = EDITS.get(hwnd) {
                let cb = slot.onMouseDown
                cb(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.lbuttonup {
            if let some(slot) = EDITS.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.mButtonDown {
            if let some(slot) = EDITS.get(hwnd) {
                let cb = slot.onMouseDown
                cb(lo, hi, 1, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.mButtonUp {
            if let some(slot) = EDITS.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 1, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.rButtonDown {
            if let some(slot) = EDITS.get(hwnd) {
                let ck = modCtrl()
                let sk = modShift()
                let ak = modAlt()
                let mk = modMeta()
                let cb = slot.onMouseDown
                cb(lo, hi, 2, ck, sk, ak, mk)
                let ctx = slot.onContextMenu
                ctx(lo, hi, 2, ck, sk, ak, mk)
            }
        }
        WindowMessage.rButtonUp {
            if let some(slot) = EDITS.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 2, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.keydown {
            if let some(slot) = EDITS.get(hwnd) {
                let vk = wParam as u32 as VirtualKey
                let cb = slot.onKeyDown
                cb(vkToString(vk), modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.keyup {
            if let some(slot) = EDITS.get(hwnd) {
                let vk = wParam as u32 as VirtualKey
                let cb = slot.onKeyUp
                cb(vkToString(vk), modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.mouseWheel {
            if let some(slot) = EDITS.get(hwnd) {
                let raw = ((wParam >> 16) & 0xFFFF) as i32
                let signed = if raw >= 0x8000 as i32 { raw - 0x10000 as i32 } else { raw }
                let dy = (signed as f64) / 120.0
                // lParam coords are screen-space on WM_MOUSEWHEEL;
                // shipping raw matches the Button-side TODO.
                let cb = slot.onMouseWheel
                cb(0.0, dy, lo, hi,
                   modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        _ {}
    }
    DefSubclassProc(hwnd as HWND, msg, wParam, lParam)
}

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
    // Subclass so per-control mouse messages reach editSubclassProc
    // before the default EDIT wndproc absorbs them.
    SetWindowSubclass(hwnd, editSubclassProc, 1 as u64, 0 as u64)
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
pub fn setOnEditFocus(handle: i64, cb: fn()) {
    if let some(slot) = EDITS.get(handle) { slot.onFocus = cb }
}
pub fn setOnEditBlur(handle: i64, cb: fn()) {
    if let some(slot) = EDITS.get(handle) { slot.onBlur = cb }
}
pub fn setOnEditMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = EDITS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnEditMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = EDITS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnEditMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = EDITS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnEditMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = EDITS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnEditContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = EDITS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnEditKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = EDITS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnEditKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = EDITS.get(handle) { slot.onKeyUp = cb }
}
pub fn setOnEditMouseWheel(
    handle: i64,
    cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = EDITS.get(handle) { slot.onMouseWheel = cb }
}
pub fn selectAllEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        // EM_SETSEL with wParam = 0 and lParam = -1 selects all.
        SendMessageW(slot.hwnd, EM_SETSEL, 0 as u64, -1 as i64)
    }
}
pub fn setEditSelection(handle: i64, start: i64, end: i64) {
    if let some(slot) = EDITS.get(handle) {
        let s = if start < 0 as i64 { 0 as i64 } else { start }
        let e = if end < s { s } else { end }
        SendMessageW(slot.hwnd, EM_SETSEL, s as u64, e as i64)
    }
}
pub fn getEditSelectionStart(handle: i64): i64 {
    if let some(slot) = EDITS.get(handle) {
        // EM_GETSEL packs (start, end) into a single DWORD return
        // (LOWORD / HIWORD). Match the same convention the API
        // uses on parameterless callers.
        let r = SendMessageW(slot.hwnd, EM_GETSEL, 0 as u64, 0 as i64) as u64
        (r & 0xFFFF) as i64
    } else { 0 as i64 }
}
pub fn getEditSelectionEnd(handle: i64): i64 {
    if let some(slot) = EDITS.get(handle) {
        let r = SendMessageW(slot.hwnd, EM_GETSEL, 0 as u64, 0 as i64) as u64
        ((r >> 16) & 0xFFFF) as i64
    } else { 0 as i64 }
}
pub fn focusEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) { SetFocus(slot.hwnd) }
}
pub fn blurEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        // Only act when this edit holds focus; otherwise we'd
        // pull it off a sibling. Punt focus to the parent window.
        if GetFocus() as i64 == slot.hwnd as i64 {
            SetFocus(slot.winHandle as HWND)
        }
    }
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
pub fn setEditPlaceholder(handle: i64, text: string) {
    if let some(slot) = EDITS.get(handle) {
        // `wParam = 1` keeps the cue visible while the field has
        // focus (matches modern Windows behaviour). The wide
        // string buffer is held in `textW` for the call's
        // duration; EM_SETCUEBANNER copies internally.
        let textW = text.encodeUtf16()
        let p: *const u16 = textW
        SendMessageW(slot.hwnd, EM_SETCUEBANNER, 1 as u64, p as i64)
    }
}
pub fn setEditReadOnly(handle: i64, readOnly: bool) {
    if let some(slot) = EDITS.get(handle) {
        let flag: u64 = if readOnly { 1 as u64 } else { 0 as u64 }
        SendMessageW(slot.hwnd, EM_SETREADONLY, flag, 0 as i64)
    }
}
pub fn setEditMaxLength(handle: i64, max: i32) {
    if let some(slot) = EDITS.get(handle) {
        SendMessageW(slot.hwnd, EM_LIMITTEXT, max as u64, 0 as i64)
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

/// `WM_COPY` / `WM_CUT` / `WM_PASTE` operate on the focused EDIT
/// control's current selection — they're posted to the control's
/// own wndproc and the standard EDIT handles the rest.
pub fn copyEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        SendMessageW(slot.hwnd, 0x0301 as u32, 0 as u64, 0 as i64)
    }
}
pub fn cutEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        SendMessageW(slot.hwnd, 0x0300 as u32, 0 as u64, 0 as i64)
    }
}
pub fn pasteEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        SendMessageW(slot.hwnd, 0x0302 as u32, 0 as u64, 0 as i64)
    }
}
/// `EM_UNDO` = 0x00C7. Win32 EDIT undoes one user-visible edit
/// step; there's no native redo so `redoEdit` simply replays
/// `EM_UNDO` (the EDIT undo stack is a single-level toggle).
pub fn undoEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        SendMessageW(slot.hwnd, 0x00C7 as u32, 0 as u64, 0 as i64)
    }
}
pub fn redoEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        SendMessageW(slot.hwnd, 0x00C7 as u32, 0 as u64, 0 as i64)
    }
}
/// `EM_SCROLLCARET` = 0x00B7.
pub fn scrollEditToCaret(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        SendMessageW(slot.hwnd, 0x00B7 as u32, 0 as u64, 0 as i64)
    }
}

} // @extern(C)
