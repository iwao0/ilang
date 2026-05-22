/// Determinate progress bar — `msctls_progress32` window class.
/// Read-only, no event surface.

use windows {
    HWND, HINSTANCE, HMENU,
    WindowStyle, ShowCmd,
    GetModuleHandleA,
    CreateWindowExW, MoveWindow, ShowWindow,
    SendMessageA,
    HeapFree, GetProcessHeap,
}
use util { toWideUtf8 }
use core { windowHwnd }

class ProgressSlot {
    pub hwnd: HWND
    pub min:  f64
    pub max:  f64
    pub init(hwnd: HWND, min: f64, max: f64) {
        this.hwnd = hwnd
        this.min  = min
        this.max  = max
    }
}

let PROGRESS: ProgressSlot[] = []

@extern(C) {

pub fn createNativeProgress(
    windowHandle: i64,
    min: f64, max: f64, value: f64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let id: i64 = PROGRESS.length as i64
    let classW = toWideUtf8("msctls_progress32") as *u16
    let emptyW = toWideUtf8("") as *u16
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    let style: u32 = (WindowStyle.child as u32) | visibleBits
    let hInstance = GetModuleHandleA(0 as *const char)
    let hwnd = CreateWindowExW(
        0,
        classW,
        emptyW,
        style,
        x as i32, y as i32, width as i32, height as i32,
        parent,
        0 as HMENU,
        hInstance as HINSTANCE,
        0 as *void
    )
    HeapFree(GetProcessHeap(), 0 as u32, classW as *void)
    HeapFree(GetProcessHeap(), 0 as u32, emptyW as *void)

    PROGRESS.push(new ProgressSlot(hwnd, min, max))
    // PBM_SETRANGE32 = 0x0406 (wParam = min, lParam = max).
    let _ = SendMessageA(hwnd, 0x0406 as u32, min as i64 as u64, max as i64)
    // PBM_SETPOS = 0x0402.
    let _ = SendMessageA(hwnd, 0x0402 as u32, value as i64 as u64, 0 as i64)
    id
}

pub fn setProgressRange(handle: i64, min: f64, max: f64) {
    if handle >= 0 && handle < (PROGRESS.length as i64) {
        PROGRESS[handle].min = min
        PROGRESS[handle].max = max
        let _ = SendMessageA(PROGRESS[handle].hwnd, 0x0406 as u32,
                             min as i64 as u64, max as i64)
    }
}
pub fn setProgressValue(handle: i64, value: f64) {
    if handle >= 0 && handle < (PROGRESS.length as i64) {
        let _ = SendMessageA(PROGRESS[handle].hwnd, 0x0402 as u32,
                             value as i64 as u64, 0 as i64)
    }
}
pub fn setProgressFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (PROGRESS.length as i64) {
        MoveWindow(PROGRESS[handle].hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setProgressVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (PROGRESS.length as i64) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(PROGRESS[handle].hwnd, cmd)
    }
}

} // @extern(C)
