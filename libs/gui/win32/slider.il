/// Numeric slider — `msctls_trackbar32` window class.
///
/// The control takes / reports integer positions; the public API
/// is in `f64` to match the cross-platform `Slider` widget, so
/// each slot caches the f64 `[min, max]` range alongside the
/// HWND. WM_HSCROLL is delivered to the parent rather than via
/// WM_COMMAND — `wnd_proc` calls `trySliderChange` from its
/// WM_HSCROLL arm.

use windows {
    HWND, HINSTANCE, HMENU, RECT,
    WindowStyle, ShowCmd, WindowMessage,
    TOOLINFOW, TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    GetModuleHandleA,
    CreateWindowExW, EnableWindow, MoveWindow, ShowWindow,
    SetFocus, GetFocus,
    SendMessageA, SendMessageW,
    SetWindowSubclass, DefSubclassProc,
}
use util { noOpF64 }
use core { windowHwnd, noOp1 }

class SliderSlot {
    pub hwnd:     HWND
    pub winHandle: i64
    pub tooltipHwnd: HWND
    pub min:      f64
    pub max:      f64
    pub onChange: fn(f64)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub init(hwnd: HWND, winHandle: i64, min: f64, max: f64) {
        this.hwnd = hwnd
        this.winHandle = winHandle
        this.tooltipHwnd = 0 as HWND
        this.min  = min
        this.max  = max
        this.onChange = noOpF64
        this.onFocus  = noOp1
        this.onBlur   = noOp1
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

pub fn sliderSubclassProc(
    hwnd: i64, msg: u32, wParam: u64, lParam: i64,
    idSubclass: u64, refData: u64
): i64 {
    match msg as WindowMessage {
        WindowMessage.setFocus {
            if let some(slot) = SLIDERS.get(hwnd) {
                let cb = slot.onFocus
                cb()
            }
        }
        WindowMessage.killFocus {
            if let some(slot) = SLIDERS.get(hwnd) {
                let cb = slot.onBlur
                cb()
            }
        }
        _ {}
    }
    DefSubclassProc(hwnd as HWND, msg, wParam, lParam)
}

pub fn createNativeSlider(
    windowHandle: i64,
    min: f64, max: f64, value: f64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = "msctls_trackbar32".encodeUtf16()
    let emptyW = "".encodeUtf16()
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
    let enabledFlag: i32 = if enabled { 1 } else { 0 }
    EnableWindow(hwnd, enabledFlag)

    let handle = hwnd as i64
    SetWindowSubclass(hwnd, sliderSubclassProc, 1 as u64, 0 as u64)
    SLIDERS[handle] = new SliderSlot(hwnd, windowHandle, min, max)
    // TBM_SETRANGE = 0x0406 (wParam = redraw, lParam = LOWORD min / HIWORD max).
    SendMessageA(hwnd, 0x0406 as u32, 1 as u64, rangeBits(min, max))
    // TBM_SETPOS = 0x0405.
    SendMessageA(hwnd, 0x0405 as u32, 1 as u64, value as i64)
    handle
}

pub fn setOnSliderChange(handle: i64, cb: fn(f64)) {
    if let some(slot) = SLIDERS.get(handle) { slot.onChange = cb }
}
pub fn setSliderRange(handle: i64, min: f64, max: f64) {
    if let some(slot) = SLIDERS.get(handle) {
        slot.min = min
        slot.max = max
        SendMessageA(slot.hwnd, 0x0406 as u32,
                             1 as u64, rangeBits(min, max))
    }
}
pub fn setSliderValue(handle: i64, value: f64) {
    if let some(slot) = SLIDERS.get(handle) {
        SendMessageA(slot.hwnd, 0x0405 as u32,
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
        MoveWindow(slot.hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setSliderEnabled(handle: i64, enabled: bool) {
    if let some(slot) = SLIDERS.get(handle) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
    }
}
pub fn setSliderVisible(handle: i64, visible: bool) {
    if let some(slot) = SLIDERS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}
pub fn setOnSliderFocus(handle: i64, cb: fn()) {
    if let some(slot) = SLIDERS.get(handle) { slot.onFocus = cb }
}
pub fn setOnSliderBlur(handle: i64, cb: fn()) {
    if let some(slot) = SLIDERS.get(handle) { slot.onBlur = cb }
}
pub fn focusSlider(handle: i64) {
    if let some(slot) = SLIDERS.get(handle) { SetFocus(slot.hwnd) }
}
pub fn blurSlider(handle: i64) {
    if let some(slot) = SLIDERS.get(handle) {
        if (GetFocus() as i64) == (slot.hwnd as i64) {
            SetFocus(slot.winHandle as HWND)
        }
    }
}
pub fn setSliderTooltip(handle: i64, text: string) {
    if let some(slot) = SLIDERS.get(handle) {
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
