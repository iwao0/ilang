/// Group box — BUTTON window class with `BS_GROUPBOX`. Titled
/// frame for visually grouping related controls; ignores clicks.

use windows {
    HWND, HINSTANCE, HMENU,
    WindowStyle, ShowCmd,
    GetModuleHandleA,
    CreateWindowExW, SetWindowTextW, MoveWindow, ShowWindow,
    HeapFree, GetProcessHeap,
}
use util { toWideUtf8 }
use core { windowHwnd }

class GroupBoxSlot {
    pub hwnd: HWND
    pub init(hwnd: HWND) { this.hwnd = hwnd }
}

let GROUPBOXES: GroupBoxSlot[] = []

@extern(C) {

pub fn createNativeGroupBox(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let id: i64 = GROUPBOXES.length as i64
    let classW = toWideUtf8("BUTTON") as *u16
    let textW  = toWideUtf8(text) as *u16
    let visibleBits: u32 = if visible {
        WindowStyle.visible as u32
    } else {
        0 as u32
    }
    // BS_GROUPBOX = 0x0007.
    let style: u32 = (WindowStyle.child as u32) | visibleBits | (0x0007 as u32)
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
    GROUPBOXES.push(new GroupBoxSlot(hwnd))
    id
}

pub fn setGroupBoxText(handle: i64, text: string) {
    if handle >= 0 && handle < (GROUPBOXES.length as i64) {
        let w = toWideUtf8(text) as *u16
        let _ = SetWindowTextW(GROUPBOXES[handle].hwnd, w)
        HeapFree(GetProcessHeap(), 0 as u32, w as *void)
    }
}
pub fn setGroupBoxFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (GROUPBOXES.length as i64) {
        MoveWindow(GROUPBOXES[handle].hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setGroupBoxVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (GROUPBOXES.length as i64) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(GROUPBOXES[handle].hwnd, cmd)
    }
}

} // @extern(C)
