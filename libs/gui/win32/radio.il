/// Radio button — BUTTON window class with `BS_AUTORADIOBUTTON`.
///
/// Mutual exclusion is handled in gui-core (the per-Window
/// `_unselectSiblings` walk) rather than by coordinating Win32
/// WS_GROUP bits across separately-created radios.

use windows {
    HWND, HINSTANCE, HMENU,
    WindowStyle, ShowCmd,
    GetModuleHandleA,
    CreateWindowExW, SetWindowTextW, EnableWindow, MoveWindow, ShowWindow,
    SendMessageA,
    HeapFree, GetProcessHeap,
}
use util { toWideUtf8, applyExplorerTheme, applyGuiFont, noOpBool }
use core { windowHwnd }

class RadioSlot {
    pub hwnd:     HWND
    pub onToggle: fn(bool)
    pub init(hwnd: HWND) {
        this.hwnd = hwnd
        this.onToggle = noOpBool
    }
}

let RADIOS = new Map<i64, RadioSlot>()

pub fn tryRadioToggle(hwnd: i64): bool {
    if let some(slot) = RADIOS.get(hwnd) {
        slot.onToggle(readRadioChecked(hwnd))
        true
    } else {
        false
    }
}

@extern(C) {

pub fn createNativeRadio(
    windowHandle: i64,
    text: string, checked: bool,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = toWideUtf8("BUTTON") as *u16
    let textW  = toWideUtf8(text) as *u16
    let visibleBits: u32 = if visible {
        WindowStyle.visible as u32
    } else {
        0 as u32
    }
    // BS_AUTORADIOBUTTON = 0x0009.
    let style: u32 = (WindowStyle.child as u32) | visibleBits | (0x0009 as u32)
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
    HeapFree(GetProcessHeap(), 0 as u32, classW as *void)
    HeapFree(GetProcessHeap(), 0 as u32, textW as *void)
    let enabledFlag: i32 = if enabled { 1 } else { 0 }
    EnableWindow(hwnd, enabledFlag)
    applyExplorerTheme(hwnd)
    applyGuiFont(hwnd)
    let checkedW: u64 = if checked { 1 as u64 } else { 0 as u64 }
    SendMessageA(hwnd, 0x00F1 as u32, checkedW, 0 as i64)
    let handle = hwnd as i64
    RADIOS[handle] = new RadioSlot(hwnd)
    handle
}

pub fn setOnRadioToggle(handle: i64, cb: fn(bool)) {
    if let some(slot) = RADIOS.get(handle) { slot.onToggle = cb }
}
pub fn setRadioText(handle: i64, text: string) {
    if let some(slot) = RADIOS.get(handle) {
        let w = toWideUtf8(text) as *u16
        SetWindowTextW(slot.hwnd, w)
        HeapFree(GetProcessHeap(), 0 as u32, w as *void)
    }
}
pub fn setRadioChecked(handle: i64, checked: bool) {
    if let some(slot) = RADIOS.get(handle) {
        let v: u64 = if checked { 1 as u64 } else { 0 as u64 }
        SendMessageA(slot.hwnd, 0x00F1 as u32, v, 0 as i64)
    }
}
pub fn readRadioChecked(handle: i64): bool {
    if let some(slot) = RADIOS.get(handle) {
        let r = SendMessageA(slot.hwnd, 0x00F0 as u32, 0 as u64, 0 as i64)
        r == 1 as i64
    } else {
        false
    }
}
pub fn setRadioFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = RADIOS.get(handle) {
        MoveWindow(slot.hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setRadioEnabled(handle: i64, enabled: bool) {
    if let some(slot) = RADIOS.get(handle) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
    }
}
pub fn setRadioVisible(handle: i64, visible: bool) {
    if let some(slot) = RADIOS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}

} // @extern(C)
