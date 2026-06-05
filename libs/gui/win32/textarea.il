/// Multi-line text input — EDIT window class + ES_MULTILINE.
///
/// Mirrors the single-line EDIT plumbing in edit.il (subclass
/// proc for mouse / key, EN_CHANGE / EN_SETFOCUS / EN_KILLFOCUS
/// from `wnd_proc.il` via `tryTextAreaChange` / `Focus` / `Blur`),
/// but with `ES_MULTILINE | ES_WANTRETURN | ES_AUTOVSCROLL |
/// WS_VSCROLL` so Enter inserts a newline and the bar gives the
/// user vertical scroll.

use windows {
    HWND, HINSTANCE, HMENU, RECT,
    WindowStyle, ShowCmd, WindowMessage, TrackMouseFlags, TRACKMOUSEEVENT,
    VirtualKey,
    TOOLINFOW, TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    GetModuleHandleA,
    CreateWindowExW, SetWindowTextW, EnableWindow, MoveWindow, ShowWindow,
    SendMessageW, SendMessageA, SetFocus, GetFocus, TrackMouseEvent,
    SetWindowSubclass, DefSubclassProc,
    GetWindowTextW, GetWindowTextLengthW,
    HeapAlloc, HeapFree, GetProcessHeap,
}
use std.ffi { arrayFromCArray }
use util {
    applyGuiFont, noOpText,
    vkToString, modCtrl, modShift, modAlt, modMeta,
}
use core { windowHwnd, noOp1 }

/// `EM_SETREADONLY` from `winuser.h`.
const EM_SETREADONLY: u32 = 0x00CF as u32

class TextAreaSlot {
    pub hwnd:     HWND
    pub winHandle: i64
    pub tooltipHwnd: HWND
    pub onChange: fn(string)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub onMouseEnter:  fn(f64, f64)
    pub onMouseLeave:  fn(f64, f64)
    pub onMouseDown:   fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:     fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onKeyDown:     fn(string, bool, bool, bool, bool)
    pub onKeyUp:       fn(string, bool, bool, bool, bool)
    pub onMouseWheel:  fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub mouseTracking: bool
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd = hwnd
        this.winHandle = winHandle
        this.tooltipHwnd = 0 as HWND
        this.onChange = noOpText
        this.onFocus  = noOp1
        this.onBlur   = noOp1
        this.onMouseEnter  = taNoOpEnterLeave
        this.onMouseLeave  = taNoOpEnterLeave
        this.onMouseDown   = taNoOpClickMod
        this.onMouseUp     = taNoOpClickMod
        this.onContextMenu = taNoOpClickMod
        this.onKeyDown     = taNoOpKey
        this.onKeyUp       = taNoOpKey
        this.onMouseWheel  = taNoOpWheel
        this.mouseTracking = false
    }
}

fn taNoOpEnterLeave(x: f64, y: f64) {}
fn taNoOpClickMod(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn taNoOpWheel(dx: f64, dy: f64, x: f64, y: f64, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn taNoOpKey(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {}

let TEXT_AREAS = new Map<i64, TextAreaSlot>()

/// Called by `wnd_proc` for WM_COMMAND / EN_CHANGE on a textarea.
pub fn tryTextAreaChange(hwnd: i64): bool {
    if let some(slot) = TEXT_AREAS.get(hwnd) {
        slot.onChange(readTextAreaText(hwnd))
        true
    } else {
        false
    }
}
pub fn tryTextAreaFocus(hwnd: i64): bool {
    if let some(slot) = TEXT_AREAS.get(hwnd) {
        slot.onFocus()
        true
    } else {
        false
    }
}
pub fn tryTextAreaBlur(hwnd: i64): bool {
    if let some(slot) = TEXT_AREAS.get(hwnd) {
        slot.onBlur()
        true
    } else {
        false
    }
}

@extern(C) {

pub fn textAreaSubclassProc(
    hwnd: i64, msg: u32, wParam: u64, lParam: i64,
    idSubclass: u64, refData: u64
): i64 {
    let lo = (lParam & 0xFFFF) as f64
    let hi = ((lParam >> 16) & 0xFFFF) as f64
    match msg as WindowMessage {
        WindowMessage.mouseMove {
            if let some(slot) = TEXT_AREAS.get(hwnd) {
                if !slot.mouseTracking {
                    let tme = TRACKMOUSEEVENT {
                        cbSize:      24 as u32,
                        dwFlags:     TrackMouseFlags.leave as u32,
                        hwndTrack:   hwnd as HWND,
                        dwHoverTime: 0 as u32,
                    }
                    TrackMouseEvent(&tme)
                    slot.mouseTracking = true
                    slot.onMouseEnter(lo, hi)
                }
            }
        }
        WindowMessage.mouseLeave {
            if let some(slot) = TEXT_AREAS.get(hwnd) {
                slot.mouseTracking = false
                slot.onMouseLeave(lo, hi)
            }
        }
        WindowMessage.lbuttondown {
            if let some(slot) = TEXT_AREAS.get(hwnd) {
                slot.onMouseDown(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.lbuttonup {
            if let some(slot) = TEXT_AREAS.get(hwnd) {
                slot.onMouseUp(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.mButtonDown {
            if let some(slot) = TEXT_AREAS.get(hwnd) {
                slot.onMouseDown(lo, hi, 1, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.mButtonUp {
            if let some(slot) = TEXT_AREAS.get(hwnd) {
                slot.onMouseUp(lo, hi, 1, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.rButtonDown {
            if let some(slot) = TEXT_AREAS.get(hwnd) {
                let ck = modCtrl()
                let sk = modShift()
                let ak = modAlt()
                let mk = modMeta()
                slot.onMouseDown(lo, hi, 2, ck, sk, ak, mk)
                let ctx = slot.onContextMenu
                ctx(lo, hi, 2, ck, sk, ak, mk)
            }
        }
        WindowMessage.rButtonUp {
            if let some(slot) = TEXT_AREAS.get(hwnd) {
                slot.onMouseUp(lo, hi, 2, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.keydown {
            if let some(slot) = TEXT_AREAS.get(hwnd) {
                let vk = wParam as u32 as VirtualKey
                slot.onKeyDown(vkToString(vk), modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.keyup {
            if let some(slot) = TEXT_AREAS.get(hwnd) {
                let vk = wParam as u32 as VirtualKey
                slot.onKeyUp(vkToString(vk), modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.mouseWheel {
            if let some(slot) = TEXT_AREAS.get(hwnd) {
                let raw = ((wParam >> 16) & 0xFFFF) as i32
                let signed = if raw >= 0x8000 as i32 { raw - 0x10000 as i32 } else { raw }
                let dy = (signed as f64) / 120.0
                slot.onMouseWheel(0.0, dy, lo, hi,
                   modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        _ {}
    }
    DefSubclassProc(hwnd as HWND, msg, wParam, lParam)
}

pub fn createNativeTextArea(
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
    // ES_MULTILINE (0x04) | ES_WANTRETURN (0x1000) |
    // ES_AUTOVSCROLL (0x40) + WS_VSCROLL (0x200000) + WS_BORDER.
    let style: u32 = (WindowStyle.child as u32)
                   | (WindowStyle.border as u32)
                   | (WindowStyle.vScroll as u32)
                   | visibleBits
                   | (0x0004 as u32)
                   | (0x1000 as u32)
                   | (0x0040 as u32)
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
    SetWindowSubclass(hwnd, textAreaSubclassProc, 1 as u64, 0 as u64)
    let handle = hwnd as i64
    TEXT_AREAS[handle] = new TextAreaSlot(hwnd, windowHandle)
    handle
}

pub fn readTextAreaText(handle: i64): string {
    if let some(slot) = TEXT_AREAS.get(handle) {
        let hwnd = slot.hwnd
        let lenW = GetWindowTextLengthW(hwnd) as i32
        if lenW <= 0 {
            ""
        } else {
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

pub fn setTextAreaText(handle: i64, text: string) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        SetWindowTextW(slot.hwnd, text.encodeUtf16())
    }
}

pub fn setTextAreaFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        MoveWindow(slot.hwnd, x as i32, y as i32, width as i32, height as i32, 1)
    }
}

pub fn setTextAreaEnabled(handle: i64, enabled: bool) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
    }
}

pub fn setTextAreaVisible(handle: i64, visible: bool) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}

pub fn setTextAreaReadOnly(handle: i64, readOnly: bool) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        let flag: u64 = if readOnly { 1 as u64 } else { 0 as u64 }
        SendMessageA(slot.hwnd, EM_SETREADONLY, flag, 0 as i64)
    }
}

pub fn focusTextArea(handle: i64) {
    if let some(slot) = TEXT_AREAS.get(handle) { SetFocus(slot.hwnd) }
}
pub fn blurTextArea(handle: i64) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        if (GetFocus() as i64) == (slot.hwnd as i64) {
            SetFocus(slot.winHandle as HWND)
        }
    }
}

pub fn setOnTextAreaChange(handle: i64, cb: fn(string)) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onChange = cb }
}
pub fn setOnTextAreaFocus(handle: i64, cb: fn()) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onFocus = cb }
}
pub fn setOnTextAreaBlur(handle: i64, cb: fn()) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onBlur = cb }
}
pub fn setOnTextAreaMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnTextAreaMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnTextAreaMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnTextAreaMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnTextAreaContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnTextAreaKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnTextAreaKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onKeyUp = cb }
}
pub fn setOnTextAreaMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onMouseWheel = cb }
}
pub fn setTextAreaTooltip(handle: i64, text: string) {
    if let some(slot) = TEXT_AREAS.get(handle) {
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
