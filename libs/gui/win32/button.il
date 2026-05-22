/// Push-button widget. Backed by the BUTTON window class.
///
/// `BN_CLICKED` notifications come in via `wnd_proc.guiWndProc`'s
/// WM_COMMAND arm; we expose `tryButtonClick(hwnd)` so the proc
/// doesn't have to reach into our `BUTTONS` map directly.

use windows {
    HWND, HINSTANCE, HMENU,
    WindowStyle, ShowCmd,
    GetModuleHandleA,
    CreateWindowExW, SetWindowTextW, EnableWindow, MoveWindow, ShowWindow,
    HeapFree, GetProcessHeap,
}
use util { toWideUtf8, applyExplorerTheme, applyGuiFont, noOpClick }
use core { windowHwnd }

/// Per-button state. Stored in `BUTTONS` keyed by the button's
/// HWND (`hwnd as i64`) — same key gui-core sees as the public
/// `handle: i64`, so both the public setters and the
/// wnd_proc-side sender lookup are O(1) Map gets.
class ButtonSlot {
    pub hwnd:    HWND
    pub onClick: fn(f64, f64, i32)
    pub init(hwnd: HWND) {
        this.hwnd = hwnd
        this.onClick = noOpClick
    }
}

let BUTTONS = new Map<i64, ButtonSlot>()

/// Called by `wnd_proc` for WM_COMMAND / BN_CLICKED. Returns
/// `true` when the HWND matched a button (so the dispatcher
/// stops walking).
pub fn tryButtonClick(hwnd: i64): bool {
    if let some(slot) = BUTTONS.get(hwnd) {
        slot.onClick(0.0, 0.0, 0)
        true
    } else {
        false
    }
}

@extern(C) {

pub fn createNativeButton(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
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
    let style: u32 = (WindowStyle.child as u32) | visibleBits
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
    applyGuiFont(hwnd as i64)
    let handle = hwnd as i64
    BUTTONS[handle] = new ButtonSlot(hwnd)
    handle
}

pub fn setOnButtonClick(handle: i64, cb: fn(f64, f64, i32)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onClick = cb }
}
pub fn setButtonText(handle: i64, text: string) {
    if let some(slot) = BUTTONS.get(handle) {
        let w = toWideUtf8(text) as *u16
        SetWindowTextW(slot.hwnd, w)
        HeapFree(GetProcessHeap(), 0 as u32, w as *void)
    }
}
pub fn setButtonFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = BUTTONS.get(handle) {
        MoveWindow(slot.hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setButtonEnabled(handle: i64, enabled: bool) {
    if let some(slot) = BUTTONS.get(handle) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
    }
}
pub fn setButtonVisible(handle: i64, visible: bool) {
    if let some(slot) = BUTTONS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}

} // @extern(C)
