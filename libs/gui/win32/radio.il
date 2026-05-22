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
use util { toWideUtf8, applyExplorerTheme, noOpBool }
use core { windowHwnd }

class RadioSlot {
    pub hwnd:     HWND
    pub onToggle: fn(bool)
    pub init(hwnd: HWND) {
        this.hwnd = hwnd
        this.onToggle = noOpBool
    }
}

let RADIOS: RadioSlot[] = []

fn findRadioSlot(hwnd: i64): i64 {
    let n = RADIOS.length
    let i: i64 = 0
    let found: i64 = -1
    while i < n {
        if RADIOS[i].hwnd as i64 == hwnd {
            found = i
            i = n
        } else {
            i = i + 1
        }
    }
    found
}

pub fn tryRadioToggle(hwnd: i64): bool {
    let ix = findRadioSlot(hwnd)
    if ix < 0 { return false }
    let cb = RADIOS[ix].onToggle
    cb(readRadioChecked(ix))
    true
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
    let id: i64 = RADIOS.length as i64
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
    applyExplorerTheme(hwnd as i64)
    let checkedW: u64 = if checked { 1 as u64 } else { 0 as u64 }
    let _ = SendMessageA(hwnd, 0x00F1 as u32, checkedW, 0 as i64)
    RADIOS.push(new RadioSlot(hwnd))
    id
}

pub fn setOnRadioToggle(handle: i64, cb: fn(bool)) {
    if handle >= 0 && handle < (RADIOS.length as i64) {
        RADIOS[handle].onToggle = cb
    }
}
pub fn setRadioText(handle: i64, text: string) {
    if handle >= 0 && handle < (RADIOS.length as i64) {
        let w = toWideUtf8(text) as *u16
        let _ = SetWindowTextW(RADIOS[handle].hwnd, w)
        HeapFree(GetProcessHeap(), 0 as u32, w as *void)
    }
}
pub fn setRadioChecked(handle: i64, checked: bool) {
    if handle >= 0 && handle < (RADIOS.length as i64) {
        let v: u64 = if checked { 1 as u64 } else { 0 as u64 }
        let _ = SendMessageA(RADIOS[handle].hwnd, 0x00F1 as u32, v, 0 as i64)
    }
}
pub fn readRadioChecked(handle: i64): bool {
    if handle < 0 || handle >= (RADIOS.length as i64) { return false }
    let r = SendMessageA(RADIOS[handle].hwnd, 0x00F0 as u32, 0 as u64, 0 as i64)
    r == 1 as i64
}
pub fn setRadioFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (RADIOS.length as i64) {
        MoveWindow(RADIOS[handle].hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setRadioEnabled(handle: i64, enabled: bool) {
    if handle >= 0 && handle < (RADIOS.length as i64) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(RADIOS[handle].hwnd, flag)
    }
}
pub fn setRadioVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (RADIOS.length as i64) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(RADIOS[handle].hwnd, cmd)
    }
}

} // @extern(C)
