/// Checkbox — BUTTON window class with `BS_AUTOCHECKBOX`.
///
/// Shares the BN_CLICKED notify code with regular buttons; the
/// `wnd_proc` walks `tryButtonClick` first, then us.

use windows {
    HWND, HINSTANCE, HMENU,
    WindowStyle, ShowCmd,
    GetModuleHandleA,
    CreateWindowExW, SetWindowTextW, EnableWindow, MoveWindow, ShowWindow,
    SendMessageA,
}
use util { applyExplorerTheme, applyGuiFont, noOpBool }
use core { windowHwnd }

class CheckboxSlot {
    pub hwnd:     HWND
    pub onToggle: fn(bool)
    pub init(hwnd: HWND) {
        this.hwnd = hwnd
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
    CHECKBOXES[handle] = new CheckboxSlot(hwnd)
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

} // @extern(C)
