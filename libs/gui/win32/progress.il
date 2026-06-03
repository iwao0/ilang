/// Determinate progress bar — `msctls_progress32` window class.
/// Read-only, no event surface.

use windows {
    HWND, HINSTANCE, HMENU, RECT,
    WindowStyle, ShowCmd, GWL,
    TOOLINFOW, TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    GetModuleHandleA,
    CreateWindowExW, EnableWindow, MoveWindow, ShowWindow,
    SendMessageA, SendMessageW,
    GetWindowLongPtrW, SetWindowLongPtrW,
}
use core { windowHwnd }

class ProgressSlot {
    pub hwnd: HWND
    pub winHandle: i64
    pub tooltipHwnd: HWND
    pub min:  f64
    pub max:  f64
    pub init(hwnd: HWND, winHandle: i64, min: f64, max: f64) {
        this.hwnd = hwnd
        this.winHandle = winHandle
        this.tooltipHwnd = 0 as HWND
        this.min  = min
        this.max  = max
    }
}

let PROGRESS = new Map<i64, ProgressSlot>()

@extern(C) {

pub fn createNativeProgress(
    windowHandle: i64,
    min: f64, max: f64, value: f64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = "msctls_progress32".encodeUtf16()
    let emptyW = "".encodeUtf16()
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
    let handle = hwnd as i64
    PROGRESS[handle] = new ProgressSlot(hwnd, windowHandle, min, max)
    // PBM_SETRANGE32 = 0x0406 (wParam = min, lParam = max).
    SendMessageA(hwnd, 0x0406 as u32, min as i64 as u64, max as i64)
    // PBM_SETPOS = 0x0402.
    SendMessageA(hwnd, 0x0402 as u32, value as i64 as u64, 0 as i64)
    handle
}

pub fn setProgressRange(handle: i64, min: f64, max: f64) {
    if let some(slot) = PROGRESS.get(handle) {
        slot.min = min
        slot.max = max
        SendMessageA(slot.hwnd, 0x0406 as u32,
                             min as i64 as u64, max as i64)
    }
}
pub fn setProgressValue(handle: i64, value: f64) {
    if let some(slot) = PROGRESS.get(handle) {
        SendMessageA(slot.hwnd, 0x0402 as u32,
                             value as i64 as u64, 0 as i64)
    }
}
pub fn setProgressFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = PROGRESS.get(handle) {
        MoveWindow(slot.hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setProgressVisible(handle: i64, visible: bool) {
    if let some(slot) = PROGRESS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}
pub fn setProgressEnabled(handle: i64, enabled: bool) {
    if let some(slot) = PROGRESS.get(handle) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
    }
}
pub fn setProgressIndeterminate(handle: i64, on: bool) {
    if let some(slot) = PROGRESS.get(handle) {
        let cur = GetWindowLongPtrW(slot.hwnd, GWL.style as i32)
        // PBS_MARQUEE = 0x08. PBM_SETMARQUEE only animates when
        // the style bit is on; flip it in / out before posting.
        let next = if on {
            cur | (0x08 as i64)
        } else {
            cur & (~(0x08 as i64))
        }
        SetWindowLongPtrW(slot.hwnd, GWL.style as i32, next)
        // PBM_SETMARQUEE = 0x040A (wParam=enable, lParam=update-ms).
        SendMessageA(
            slot.hwnd, 0x040A as u32,
            if on { 1 as u64 } else { 0 as u64 },
            30 as i64
        )
    }
}
pub fn setProgressOrientation(handle: i64, vertical: bool) {
    if let some(slot) = PROGRESS.get(handle) {
        let cur = GetWindowLongPtrW(slot.hwnd, GWL.style as i32)
        // PBS_VERTICAL = 0x04.
        let next = if vertical {
            cur | (0x04 as i64)
        } else {
            cur & (~(0x04 as i64))
        }
        SetWindowLongPtrW(slot.hwnd, GWL.style as i32, next)
    }
}
pub fn setProgressTooltip(handle: i64, text: string) {
    if let some(slot) = PROGRESS.get(handle) {
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
