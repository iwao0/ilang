/// Label widget — STATIC window class. Display-only, no events.

use windows {
    HWND, HINSTANCE, HMENU,
    WindowStyle, ShowCmd,
    GetModuleHandleA,
    CreateWindowExW, SetWindowTextW, MoveWindow, ShowWindow,
}
use util { applyGuiFont }
use core { windowHwnd }

class LabelSlot {
    pub hwnd: HWND
    pub init(hwnd: HWND) { this.hwnd = hwnd }
}

let LABELS = new Map<i64, LabelSlot>()

@extern(C) {

pub fn createNativeLabel(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = "STATIC".encodeUtf16()
    let textW  = text.encodeUtf16()
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
        0 as HMENU,            // labels emit no commands
        hInstance as HINSTANCE,
        0 as *void
    )
    applyGuiFont(hwnd)
    let handle = hwnd as i64
    LABELS[handle] = new LabelSlot(hwnd)
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

} // @extern(C)
