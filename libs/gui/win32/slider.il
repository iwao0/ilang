/// Numeric slider — `msctls_trackbar32` window class.
///
/// The control takes / reports integer positions; the public API
/// is in `f64` to match the cross-platform `Slider` widget, so
/// each slot caches the f64 `[min, max]` range alongside the
/// HWND. WM_HSCROLL is delivered to the parent rather than via
/// WM_COMMAND — `wnd_proc` calls `trySliderChange` from its
/// WM_HSCROLL arm.

use windows {
    HWND, HINSTANCE, HMENU,
    WindowStyle, ShowCmd,
    GetModuleHandleA,
    CreateWindowExW, EnableWindow, MoveWindow, ShowWindow,
    SendMessageA,
    HeapFree, GetProcessHeap,
}
use util { toWideUtf8, noOpF64 }
use core { windowHwnd }

class SliderSlot {
    pub hwnd:     HWND
    pub min:      f64
    pub max:      f64
    pub onChange: fn(f64)
    pub init(hwnd: HWND, min: f64, max: f64) {
        this.hwnd = hwnd
        this.min  = min
        this.max  = max
        this.onChange = noOpF64
    }
}

let SLIDERS = new Map<i64, SliderSlot>()

pub fn trySliderChange(hwnd: i64): bool {
    if let some(slot) = SLIDERS.get(hwnd) {
        slot.onChange(readSliderValue(hwnd))
        true
    } else {
        false
    }
}

fn rangeBits(min: f64, max: f64): i64 {
    let mn = min as i32
    let mx = max as i32
    (mn as i64 & 0xFFFF) | ((mx as i64 & 0xFFFF) << 16)
}

@extern(C) {

pub fn createNativeSlider(
    windowHandle: i64,
    min: f64, max: f64, value: f64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = toWideUtf8("msctls_trackbar32") as *u16
    let emptyW = toWideUtf8("") as *u16
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    // TBS_AUTOTICKS = 0x0001 — automatic tick marks.
    let style: u32 = (WindowStyle.child as u32) | visibleBits | (0x0001 as u32)
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
    let enabledFlag: i32 = if enabled { 1 } else { 0 }
    EnableWindow(hwnd, enabledFlag)

    let handle = hwnd as i64
    SLIDERS[handle] = new SliderSlot(hwnd, min, max)
    // TBM_SETRANGE = 0x0406 (wParam = redraw, lParam = LOWORD min / HIWORD max).
    let _ = SendMessageA(hwnd, 0x0406 as u32, 1 as u64, rangeBits(min, max))
    // TBM_SETPOS = 0x0405.
    let _ = SendMessageA(hwnd, 0x0405 as u32, 1 as u64, value as i64)
    handle
}

pub fn setOnSliderChange(handle: i64, cb: fn(f64)) {
    if let some(slot) = SLIDERS.get(handle) { slot.onChange = cb }
}
pub fn setSliderRange(handle: i64, min: f64, max: f64) {
    if let some(slot) = SLIDERS.get(handle) {
        slot.min = min
        slot.max = max
        let _ = SendMessageA(slot.hwnd, 0x0406 as u32,
                             1 as u64, rangeBits(min, max))
    }
}
pub fn setSliderValue(handle: i64, value: f64) {
    if let some(slot) = SLIDERS.get(handle) {
        let _ = SendMessageA(slot.hwnd, 0x0405 as u32,
                             1 as u64, value as i64)
    }
}
/// TBM_GETPOS = 0x0400.
pub fn readSliderValue(handle: i64): f64 {
    if let some(slot) = SLIDERS.get(handle) {
        let r = SendMessageA(slot.hwnd, 0x0400 as u32, 0 as u64, 0 as i64)
        r as f64
    } else {
        0.0
    }
}
pub fn setSliderFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = SLIDERS.get(handle) {
        let _ = MoveWindow(slot.hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setSliderEnabled(handle: i64, enabled: bool) {
    if let some(slot) = SLIDERS.get(handle) {
        let flag: i32 = if enabled { 1 } else { 0 }
        let _ = EnableWindow(slot.hwnd, flag)
    }
}
pub fn setSliderVisible(handle: i64, visible: bool) {
    if let some(slot) = SLIDERS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        let _ = ShowWindow(slot.hwnd, cmd)
    }
}

} // @extern(C)
