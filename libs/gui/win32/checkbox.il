/// Checkbox — BUTTON window class with `BS_AUTOCHECKBOX`.
///
/// Shares the BN_CLICKED notify code with regular buttons; the
/// `wnd_proc` walks `tryButtonClick` first, then us.

use windows {
    HWND, HINSTANCE, HMENU, RECT,
    WindowStyle, ShowCmd,
    TOOLINFOW, TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    GetModuleHandleA,
    CreateWindowExW, SetWindowTextW, EnableWindow, MoveWindow, ShowWindow,
    SendMessageA, SendMessageW,
}
use util { applyExplorerTheme, applyGuiFont, noOpBool }
use core { windowHwnd }

class CheckboxSlot {
    pub hwnd:     HWND
    pub winHandle: i64
    pub tooltipHwnd: HWND
    pub onToggle: fn(bool)
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd = hwnd
        this.winHandle = winHandle
        this.tooltipHwnd = 0 as HWND
        this.onToggle = noOpBool
    }
}

let CHECKBOXES = new Map<i64, CheckboxSlot>()

pub fn tryCheckboxToggle(hwnd: i64): bool {
    if let some(slot) = CHECKBOXES.get(hwnd) {
        slot.onToggle(readCheckboxChecked(hwnd))
        true
    } else {
        false
    }
}

@extern(C) {

pub fn createNativeCheckbox(
    windowHandle: i64,
    text: string, checked: bool,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = "BUTTON".encodeUtf16()
    let textW  = text.encodeUtf16()
    let visibleBits: u32 = if visible {
        WindowStyle.visible as u32
    } else {
        0 as u32
    }
    // BS_AUTOCHECKBOX = 0x0003.
    let style: u32 = (WindowStyle.child as u32) | visibleBits | (0x0003 as u32)
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
    applyExplorerTheme(hwnd)
    applyGuiFont(hwnd)
    // BM_SETCHECK = 0x00F1.
    let checkedW: u64 = if checked { 1 as u64 } else { 0 as u64 }
    SendMessageA(hwnd, 0x00F1 as u32, checkedW, 0 as i64)
    let handle = hwnd as i64
    CHECKBOXES[handle] = new CheckboxSlot(hwnd, windowHandle)
    handle
}

pub fn setOnCheckboxToggle(handle: i64, cb: fn(bool)) {
    if let some(slot) = CHECKBOXES.get(handle) { slot.onToggle = cb }
}
pub fn setCheckboxText(handle: i64, text: string) {
    if let some(slot) = CHECKBOXES.get(handle) {
        SetWindowTextW(slot.hwnd, text.encodeUtf16())
    }
}
pub fn setCheckboxChecked(handle: i64, checked: bool) {
    if let some(slot) = CHECKBOXES.get(handle) {
        let v: u64 = if checked { 1 as u64 } else { 0 as u64 }
        SendMessageA(slot.hwnd, 0x00F1 as u32, v, 0 as i64)
    }
}
/// BM_GETCHECK = 0x00F0; returns BST_CHECKED (1) when ticked.
pub fn readCheckboxChecked(handle: i64): bool {
    if let some(slot) = CHECKBOXES.get(handle) {
        let r = SendMessageA(slot.hwnd, 0x00F0 as u32, 0 as u64, 0 as i64)
        r == 1 as i64
    } else {
        false
    }
}
pub fn setCheckboxFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = CHECKBOXES.get(handle) {
        MoveWindow(slot.hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setCheckboxEnabled(handle: i64, enabled: bool) {
    if let some(slot) = CHECKBOXES.get(handle) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
    }
}
pub fn setCheckboxVisible(handle: i64, visible: bool) {
    if let some(slot) = CHECKBOXES.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}
pub fn setCheckboxTooltip(handle: i64, text: string) {
    if let some(slot) = CHECKBOXES.get(handle) {
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
