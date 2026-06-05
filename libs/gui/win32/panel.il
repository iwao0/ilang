/// Panel — bare transparent container HWND. Just a `STATIC` class
/// child window with no caption and no border: it accepts other
/// controls as children (so a `Panel` is a real parenting host,
/// not a logical-only abstraction) but draws nothing of its own.

use windows {
    HWND, HINSTANCE, HMENU,
    WindowStyle, ShowCmd,
    GetModuleHandleA,
    CreateWindowExW, MoveWindow, ShowWindow,
}
use core { windowHwnd }

@extern(C) {

pub fn createNativePanel(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = "STATIC".encodeUtf16()
    let titleW = "".encodeUtf16()
    let visibleBits: u32 = if visible {
        WindowStyle.visible as u32
    } else {
        0 as u32
    }
    let style: u32 = (WindowStyle.child as u32) | visibleBits
    let hInstance = GetModuleHandleA(0 as *const char)
    let hwnd = CreateWindowExW(
        0, classW, titleW, style,
        x as i32, y as i32, width as i32, height as i32,
        parent, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
    )
    hwnd as i64
}

pub fn setPanelFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle <= 0 as i64 { return }
    MoveWindow(handle as HWND, x as i32, y as i32, width as i32, height as i32, 1)
}

pub fn setPanelVisible(handle: i64, visible: bool) {
    if handle <= 0 as i64 { return }
    let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
    ShowWindow(handle as HWND, cmd)
}

} // @extern(C)
