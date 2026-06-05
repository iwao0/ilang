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
    WindowStyle, ShowCmd, WindowMessage, TrackMouseFlags, TRACKMOUSEEVENT,
    VirtualKey,
    TOOLINFOW, TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    GetModuleHandleA,
    CreateWindowExW, EnableWindow, MoveWindow, ShowWindow,
    SetFocus, GetFocus, TrackMouseEvent,
    SendMessageA, SendMessageW,
    SetWindowSubclass, DefSubclassProc,
}
use util {
    noOpF64,
    vkToString, modCtrl, modShift, modAlt, modMeta,
}
use core { windowHwnd, noOp1, noOpEnterLeave, noOpClickMod, noOpWheel, noOpKey }

class SliderSlot {
    pub hwnd:     HWND
    pub winHandle: i64
    pub tooltipHwnd: HWND
    pub min:      f64
    pub max:      f64
    pub onChange:  fn(f64)
    pub onRelease: fn(f64)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub onKeyDown: fn(string, bool, bool, bool, bool)
    pub onKeyUp:   fn(string, bool, bool, bool, bool)
    pub mouseTracking: bool
    pub init(hwnd: HWND, winHandle: i64, min: f64, max: f64) {
        this.hwnd = hwnd
        this.winHandle = winHandle
        this.tooltipHwnd = 0 as HWND
        this.min  = min
        this.max  = max
        this.onChange  = noOpF64
        this.onRelease = noOpF64
        this.onFocus  = noOp1
        this.onBlur   = noOp1
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseDown  = noOpClickMod
        this.onMouseUp    = noOpClickMod
        this.onContextMenu = noOpClickMod
        this.onMouseWheel = noOpWheel
        this.onKeyDown = noOpKey
        this.onKeyUp   = noOpKey
        this.mouseTracking = false
    }
}


let SLIDERS = new Map<i64, SliderSlot>()

/// WM_HSCROLL → Slider. `scrollCode` is LOWORD(wParam) — the
/// trackbar action that triggered the message. TB_ENDTRACK (0x08)
/// is the drag-end commit, so on top of the per-move `onChange`
/// we fire `onRelease` once with the final value.
pub fn trySliderChange(hwnd: i64, scrollCode: i32): bool {
    if let some(slot) = SLIDERS.get(hwnd) {
        let v = readSliderValue(hwnd)
        slot.onChange(v)
        if scrollCode == 0x08 as i32 {
            slot.onRelease(v)
        }
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
    let lo = (lParam & 0xFFFF) as f64
    let hi = ((lParam >> 16) & 0xFFFF) as f64
    match msg as WindowMessage {
        WindowMessage.setFocus {
            if let some(slot) = SLIDERS.get(hwnd) {
                slot.onFocus()
            }
        }
        WindowMessage.killFocus {
            if let some(slot) = SLIDERS.get(hwnd) {
                slot.onBlur()
            }
        }
        WindowMessage.mouseMove {
            if let some(slot) = SLIDERS.get(hwnd) {
                if !slot.mouseTracking {
                    let tme = TRACKMOUSEEVENT {
                        cbSize:      24 as u32,
                        dwFlags:     TrackMouseFlags.leave as u32,
                        hwndTrack:   hwnd as HWND,
                        dwHoverTime: 0 as u32,
                    }
                    TrackMouseEvent(&tme)
                    slot.mouseTracking = true
                    slot.onMouseEnter(lo, hi)
                }
            }
        }
        WindowMessage.mouseLeave {
            if let some(slot) = SLIDERS.get(hwnd) {
                slot.mouseTracking = false
                slot.onMouseLeave(lo, hi)
            }
        }
        WindowMessage.lbuttondown {
            if let some(slot) = SLIDERS.get(hwnd) {
                slot.onMouseDown(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.lbuttonup {
            if let some(slot) = SLIDERS.get(hwnd) {
                slot.onMouseUp(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.mButtonDown {
            if let some(slot) = SLIDERS.get(hwnd) {
                slot.onMouseDown(lo, hi, 1, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.mButtonUp {
            if let some(slot) = SLIDERS.get(hwnd) {
                slot.onMouseUp(lo, hi, 1, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.rButtonDown {
            if let some(slot) = SLIDERS.get(hwnd) {
                let ck = modCtrl()
                let sk = modShift()
                let ak = modAlt()
                let mk = modMeta()
                slot.onMouseDown(lo, hi, 2, ck, sk, ak, mk)
                let ctx = slot.onContextMenu
                ctx(lo, hi, 2, ck, sk, ak, mk)
            }
        }
        WindowMessage.rButtonUp {
            if let some(slot) = SLIDERS.get(hwnd) {
                slot.onMouseUp(lo, hi, 2, modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.mouseWheel {
            if let some(slot) = SLIDERS.get(hwnd) {
                let raw = ((wParam >> 16) & 0xFFFF) as i32
                let signed = if raw >= 0x8000 as i32 { raw - 0x10000 as i32 } else { raw }
                let dy = (signed as f64) / 120.0
                slot.onMouseWheel(0.0, dy, lo, hi,
                   modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.keydown {
            if let some(slot) = SLIDERS.get(hwnd) {
                let vk = wParam as u32 as VirtualKey
                slot.onKeyDown(vkToString(vk), modCtrl(), modShift(), modAlt(), modMeta())
            }
        }
        WindowMessage.keyup {
            if let some(slot) = SLIDERS.get(hwnd) {
                let vk = wParam as u32 as VirtualKey
                slot.onKeyUp(vkToString(vk), modCtrl(), modShift(), modAlt(), modMeta())
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
pub fn setOnSliderRelease(handle: i64, cb: fn(f64)) {
    if let some(slot) = SLIDERS.get(handle) { slot.onRelease = cb }
}
/// TBM_SETTICFREQ = 0x0414 (wParam = tick frequency, lParam = 0).
/// The trackbar stores positions as ints, so `interval` is rounded
/// to the nearest integer step; `0.0` clears the auto-ticks pattern.
pub fn setSliderTickInterval(handle: i64, min: f64, max: f64, interval: f64) {
    if let some(slot) = SLIDERS.get(handle) {
        let freq = interval as i32
        let f: i32 = if freq < 1 as i32 { 1 as i32 } else { freq }
        SendMessageA(slot.hwnd, 0x0414 as u32, f as u64, 0 as i64)
    }
}
/// TBM_SETPAGESIZE = 0x0415 (wParam = 0, lParam = pagesize).
pub fn setSliderPageStep(handle: i64, step: f64) {
    if let some(slot) = SLIDERS.get(handle) {
        SendMessageA(slot.hwnd, 0x0415 as u32, 0 as u64, step as i64)
    }
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
pub fn setOnSliderMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = SLIDERS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnSliderMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = SLIDERS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnSliderMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = SLIDERS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnSliderMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = SLIDERS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnSliderContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = SLIDERS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnSliderMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = SLIDERS.get(handle) { slot.onMouseWheel = cb }
}
pub fn setOnSliderKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = SLIDERS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnSliderKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = SLIDERS.get(handle) { slot.onKeyUp = cb }
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
