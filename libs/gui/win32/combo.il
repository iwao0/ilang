/// Combo box (dropdown) — COMBOBOX window class with
/// `CBS_DROPDOWNLIST`. CBN_SELCHANGE = notify 1 (shared with
/// LBN_SELCHANGE) — `wnd_proc` calls `tryComboSelect` first.

use windows {
    HWND, HINSTANCE, HMENU, RECT,
    WindowStyle, ShowCmd,
    TOOLINFOW, TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    GetModuleHandleA,
    CreateWindowExW, EnableWindow, MoveWindow, ShowWindow,
    SendMessageA, SendMessageW,
}
use util { applyGuiFont, noOpI32 }
use core { windowHwnd }

class ComboSlot {
    pub hwnd:     HWND
    pub winHandle: i64
    pub tooltipHwnd: HWND
    pub onSelect: fn(i32)
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd = hwnd
        this.winHandle = winHandle
        this.tooltipHwnd = 0 as HWND
        this.onSelect = noOpI32
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
