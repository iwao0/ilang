/// Combo box (dropdown) — COMBOBOX window class with
/// `CBS_DROPDOWNLIST`. CBN_SELCHANGE = notify 1 (shared with
/// LBN_SELCHANGE) — `wnd_proc` calls `tryComboSelect` first.

use windows {
    HWND, HINSTANCE, HMENU, RECT,
    WindowStyle, ShowCmd, WindowMessage, TrackMouseFlags, TRACKMOUSEEVENT,
    VirtualKey,
    TOOLINFOW, TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    GetModuleHandleA,
    CreateWindowExW, EnableWindow, MoveWindow, ShowWindow,
    SetFocus, GetFocus, TrackMouseEvent,
    SendMessageA, SendMessageW,
    SetWindowSubclass, DefSubclassProc,
}
use util {
    applyGuiFont, noOpI32,
    vkToString, modCtrl, modShift, modAlt, modMeta,
}
use core { windowHwnd, noOp1, noOpEnterLeave, noOpClickMod, noOpWheel, noOpKey }

class ComboSlot {
    pub hwnd:     HWND
    pub winHandle: i64
    pub tooltipHwnd: HWND
    pub onSelect: fn(i32)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub onKeyDown: fn(string, bool, bool, bool, bool)
    pub onKeyUp:   fn(string, bool, bool, bool, bool)
    pub mouseTracking: bool
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd = hwnd
        this.winHandle = winHandle
        this.tooltipHwnd = 0 as HWND
        this.onSelect = noOpI32
        this.onFocus  = noOp1
        this.onBlur   = noOp1
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseDown  = noOpClickMod
        this.onMouseUp    = noOpClickMod
        this.onContextMenu = noOpClickMod
        this.onMouseWheel = noOpWheel
        this.onKeyDown = noOpKey
        this.onKeyUp   = noOpKey
        this.mouseTracking = false
    }
}


let COMBOS = new Map<i64, ComboSlot>()

pub fn tryComboSelect(hwnd: i64): bool {
    if let some(slot) = COMBOS.get(hwnd) {
        slot.onSelect(readComboSelectedIndex(hwnd))
        true
    } else {
        false
    }
}

@extern(C) {

pub fn comboSubclassProc(
    hwnd: i64, msg: u32, wParam: u64, lParam: i64,
    idSubclass: u64, refData: u64
): i64 {
    let lo = (lParam & 0xFFFF) as f64
    let hi = ((lParam >> 16) & 0xFFFF) as f64
    match msg as WindowMessage {
        WindowMessage.setFocus {
            if let some(slot) = COMBOS.get(hwnd) {
                let cb = slot.onFocus
                cb()
            }
        }
        WindowMessage.killFocus {
            if let some(slot) = COMBOS.get(hwnd) {
                let cb = slot.onBlur
                cb()
            }
        }
        WindowMessage.mouseMove {
            if let some(slot) = COMBOS.get(hwnd) {
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
            if let some(slot) = COMBOS.get(hwnd) {
                slot.mouseTracking = false
                let cb = slot.onMouseLeave
                cb(lo, hi)
            }
        }
        WindowMessage.lbuttondown {
            if let some(slot) = COMBOS.get(hwnd) {
                let cb = slot.onMouseDown
                cb(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.lbuttonup {
            if let some(slot) = COMBOS.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.mButtonDown {
            if let some(slot) = COMBOS.get(hwnd) {
                let cb = slot.onMouseDown
                cb(lo, hi, 1, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.mButtonUp {
            if let some(slot) = COMBOS.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 1, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.rButtonDown {
            if let some(slot) = COMBOS.get(hwnd) {
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
            if let some(slot) = COMBOS.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 2, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.mouseWheel {
            if let some(slot) = COMBOS.get(hwnd) {
                let raw = ((wParam >> 16) & 0xFFFF) as i32
                let signed = if raw >= 0x8000 as i32 { raw - 0x10000 as i32 } else { raw }
                let dy = (signed as f64) / 120.0
                let cb = slot.onMouseWheel
                cb(0.0, dy, lo, hi,
                   modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.keydown {
            if let some(slot) = COMBOS.get(hwnd) {
                let vk = wParam as u32 as VirtualKey
                let cb = slot.onKeyDown
                cb(vkToString(vk), modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.keyup {
            if let some(slot) = COMBOS.get(hwnd) {
                let vk = wParam as u32 as VirtualKey
                let cb = slot.onKeyUp
                cb(vkToString(vk), modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        _ {}
    }
    DefSubclassProc(hwnd as HWND, msg, wParam, lParam)
}

pub fn createNativeCombo(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = "COMBOBOX".encodeUtf16()
    let emptyW = "".encodeUtf16()
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    // CBS_DROPDOWNLIST = 0x0003, WS_VSCROLL for tall lists.
    let style: u32 = (WindowStyle.child as u32) | visibleBits
                   | (0x0003 as u32)
                   | (WindowStyle.vScroll as u32)
    let hInstance = GetModuleHandleA(0 as *const char)
    // Win32 takes (closed box + dropped list) as one rect; add
    // ~120 px so the popup is actually usable.
    let totalH = (height as i32) + 120
    let hwnd = CreateWindowExW(
        0,
        classW,
        emptyW,
        style,
        x as i32, y as i32, width as i32, totalH,
        parent,
        0 as HMENU,
        hInstance as HINSTANCE,
        0 as *void
    )
    let enabledFlag: i32 = if enabled { 1 } else { 0 }
    EnableWindow(hwnd, enabledFlag)
    applyGuiFont(hwnd)
    let handle = hwnd as i64
    SetWindowSubclass(hwnd, comboSubclassProc, 1 as u64, 0 as u64)
    COMBOS[handle] = new ComboSlot(hwnd, windowHandle)
    handle
}

pub fn setOnComboSelect(handle: i64, cb: fn(i32)) {
    if let some(slot) = COMBOS.get(handle) { slot.onSelect = cb }
}
/// CB_ADDSTRING = 0x143 (wide variant for non-ASCII items).
pub fn addComboItem(handle: i64, item: string) {
    if let some(slot) = COMBOS.get(handle) {
        // The lParam is an `i64` channel; bind through a typed
        // pointer so the implicit `u16[] → *const u16` coercion
        // fires, then cast that to `i64`. `arr` stays in scope so
        // ARC keeps the buffer alive for the duration of the call.
        let arr = item.encodeUtf16()
        let p: *const u16 = arr
        SendMessageW(slot.hwnd, 0x0143 as u32, 0 as u64, p as i64)
    }
}
/// CB_RESETCONTENT = 0x14B.
pub fn clearComboItems(handle: i64) {
    if let some(slot) = COMBOS.get(handle) {
        SendMessageA(slot.hwnd, 0x014B as u32,
                             0 as u64, 0 as i64)
    }
}
/// CB_SETCURSEL = 0x14E.
pub fn setComboSelectedIndex(handle: i64, index: i32) {
    if let some(slot) = COMBOS.get(handle) {
        SendMessageA(slot.hwnd, 0x014E as u32,
                             index as i64 as u64, 0 as i64)
    }
}
/// CB_GETCURSEL = 0x147; returns `CB_ERR = -1` for no selection.
pub fn readComboSelectedIndex(handle: i64): i32 {
    if let some(slot) = COMBOS.get(handle) {
        let r = SendMessageA(slot.hwnd, 0x0147 as u32, 0 as u64, 0 as i64)
        r as i32
    } else {
        -1 as i32
    }
}
pub fn setComboFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = COMBOS.get(handle) {
        let totalH = (height as i32) + 120
        MoveWindow(slot.hwnd,
                   x as i32, y as i32, width as i32, totalH, 1)
    }
}
pub fn setComboEnabled(handle: i64, enabled: bool) {
    if let some(slot) = COMBOS.get(handle) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
    }
}
pub fn setComboVisible(handle: i64, visible: bool) {
    if let some(slot) = COMBOS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}
pub fn setOnComboFocus(handle: i64, cb: fn()) {
    if let some(slot) = COMBOS.get(handle) { slot.onFocus = cb }
}
pub fn setOnComboBlur(handle: i64, cb: fn()) {
    if let some(slot) = COMBOS.get(handle) { slot.onBlur = cb }
}
pub fn setOnComboMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = COMBOS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnComboMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = COMBOS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnComboMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = COMBOS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnComboMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = COMBOS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnComboContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = COMBOS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnComboMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = COMBOS.get(handle) { slot.onMouseWheel = cb }
}
pub fn setOnComboKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = COMBOS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnComboKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = COMBOS.get(handle) { slot.onKeyUp = cb }
}
pub fn focusCombo(handle: i64) {
    if let some(slot) = COMBOS.get(handle) { SetFocus(slot.hwnd) }
}
pub fn blurCombo(handle: i64) {
    if let some(slot) = COMBOS.get(handle) {
        if (GetFocus() as i64) == (slot.hwnd as i64) {
            SetFocus(slot.winHandle as HWND)
        }
    }
}
pub fn setComboTooltip(handle: i64, text: string) {
    if let some(slot) = COMBOS.get(handle) {
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
