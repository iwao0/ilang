/// Label widget — STATIC window class.

use windows {
    HWND, HINSTANCE, HMENU, RECT,
    WindowStyle, ShowCmd, WindowMessage, TrackMouseFlags, TRACKMOUSEEVENT,
    TOOLINFOW, TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    GetModuleHandleA,
    CreateWindowExW, SetWindowTextW, MoveWindow, ShowWindow,
    EnableWindow,
    SendMessageW, TrackMouseEvent,
    SetWindowSubclass, DefSubclassProc,
}
use util {
    applyGuiFont,
    modCtrl, modShift, modAlt, modMeta,
}
use core { windowHwnd, noOpEnterLeave, noOpClickMod, noOpWheel, noOpKey }

/// SS_NOTIFY — make the STATIC control accept mouse hits instead
/// of returning HTTRANSPARENT (the default), so the subclass proc
/// actually sees WM_*BUTTON* / WM_MOUSE* messages.
const SS_NOTIFY: u32 = 0x00000100 as u32

class LabelSlot {
    pub hwnd: HWND
    pub winHandle: i64
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)
    /// Win32 has no native `WM_MOUSEENTER`; we synthesise it the
    /// first time `WM_MOUSEMOVE` arrives after a previous leave.
    pub mouseTracking: bool
    /// Tooltip control owned by this label. Lazy-created on the
    /// first `setLabelTooltip(_, non-empty)` call; reused for
    /// subsequent updates via `TTM_UPDATETIPTEXTW`. Stays `0` (the
    /// HWND nullary) while no tooltip is configured.
    pub tooltipHwnd: HWND
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd = hwnd
        this.winHandle = winHandle
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseDown  = noOpClickMod
        this.onMouseUp    = noOpClickMod
        this.onContextMenu = noOpClickMod
        this.onMouseWheel = noOpWheel
        this.mouseTracking = false
        this.tooltipHwnd = 0 as HWND
    }
}


let LABELS = new Map<i64, LabelSlot>()

@extern(C) {

/// Window-procedure subclass installed on every STATIC instance.
/// Surfaces per-control mouse events the default STATIC wndproc
/// would otherwise ignore. Forwards everything through
/// `DefSubclassProc` so paint / size etc. stay intact.
pub fn labelSubclassProc(
    hwnd: i64, msg: u32, wParam: u64, lParam: i64,
    idSubclass: u64, refData: u64
): i64 {
    let lo = (lParam & 0xFFFF) as f64
    let hi = ((lParam >> 16) & 0xFFFF) as f64
    match msg as WindowMessage {
        WindowMessage.mouseMove {
            if let some(slot) = LABELS.get(hwnd) {
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
            if let some(slot) = LABELS.get(hwnd) {
                slot.mouseTracking = false
                let cb = slot.onMouseLeave
                cb(lo, hi)
            }
        }
        WindowMessage.lbuttondown {
            if let some(slot) = LABELS.get(hwnd) {
                let cb = slot.onMouseDown
                cb(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.lbuttonup {
            if let some(slot) = LABELS.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.mButtonDown {
            if let some(slot) = LABELS.get(hwnd) {
                let cb = slot.onMouseDown
                cb(lo, hi, 1, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.mButtonUp {
            if let some(slot) = LABELS.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 1, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.rButtonDown {
            if let some(slot) = LABELS.get(hwnd) {
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
            if let some(slot) = LABELS.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 2, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.mouseWheel {
            if let some(slot) = LABELS.get(hwnd) {
                let raw = ((wParam >> 16) & 0xFFFF) as i32
                let signed = if raw >= 0x8000 as i32 { raw - 0x10000 as i32 } else { raw }
                let dy = (signed as f64) / 120.0
                let cb = slot.onMouseWheel
                cb(0.0, dy, lo, hi,
                   modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        _ {}
    }
    DefSubclassProc(hwnd as HWND, msg, wParam, lParam)
}

pub fn createNativeLabel(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool, selectable: bool
): i64 {
    // `selectable` is honoured on macOS (NSTextField), no-op here:
    // Win32 STATIC controls don't carry a selection state.
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = "STATIC".encodeUtf16()
    let textW  = text.encodeUtf16()
    let visibleBits: u32 = if visible {
        WindowStyle.visible as u32
    } else {
        0 as u32
    }
    let disabledBits: u32 = if !enabled {
        WindowStyle.disabled as u32
    } else {
        0 as u32
    }
    // `SS_NOTIFY` is what makes STATIC accept mouse hits in the
    // first place — without it the control returns HTTRANSPARENT
    // and the subclass proc never sees WM_*BUTTON* messages.
    let style: u32 = (WindowStyle.child as u32) | visibleBits | disabledBits | SS_NOTIFY
    let hInstance = GetModuleHandleA(0 as *const char)
    let hwnd = CreateWindowExW(
        0,
        classW,
        textW,
        style,
        x as i32, y as i32, width as i32, height as i32,
        parent,
        0 as HMENU,            // labels emit no commands
        hInstance as HINSTANCE,
        0 as *void
    )
    applyGuiFont(hwnd)
    SetWindowSubclass(hwnd, labelSubclassProc, 1 as u64, 0 as u64)
    let handle = hwnd as i64
    LABELS[handle] = new LabelSlot(hwnd, windowHandle)
    handle
}

pub fn setLabelText(handle: i64, text: string) {
    if let some(slot) = LABELS.get(handle) {
        SetWindowTextW(slot.hwnd, text.encodeUtf16())
    }
}
pub fn setLabelFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = LABELS.get(handle) {
        MoveWindow(slot.hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setLabelVisible(handle: i64, visible: bool) {
    if let some(slot) = LABELS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}
pub fn setLabelEnabled(handle: i64, enabled: bool) {
    if let some(slot) = LABELS.get(handle) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
    }
}
pub fn setLabelSelectable(handle: i64, selectable: bool) {
    // No-op on Win32 — STATIC controls don't carry a selection state.
}
pub fn setLabelTooltip(handle: i64, text: string) {
    if let some(slot) = LABELS.get(handle) {
        // Empty string → delete the tool from the tooltip control
        // (if one exists). The tooltip HWND is left alive so
        // flipping back to a non-empty string reuses it.
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
        // `textW` must outlive the SendMessageW call: TOOLINFOW
        // stores a borrowed pointer that the tooltip control reads
        // before copying internally.
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

pub fn setOnLabelMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = LABELS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnLabelMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = LABELS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnLabelMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = LABELS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnLabelMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = LABELS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnLabelContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = LABELS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnLabelMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = LABELS.get(handle) { slot.onMouseWheel = cb }
}

} // @extern(C)
