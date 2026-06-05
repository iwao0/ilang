/// Switch — on/off toggle. Win32 has no native switch widget
/// (`Windows.UI.Xaml.Controls.ToggleSwitch` is UWP only), so this
/// is a custom-drawn child window in its own `IlangGuiSwitch`
/// class, rendered with GDI primitives and rebuilt on every
/// `WM_PAINT`.
///
/// Geometry: a pill-shaped track (rounded rect, corner radius =
/// height/2) with a circular thumb that snaps left (off) or
/// right (on). Hit-testing is "click anywhere in the client area
/// to toggle"; keyboard activation is Space / Enter.

use windows {
    HWND, HDC, HBRUSH, HPEN, HGDIOBJ,
    HINSTANCE, HMENU, HICON, HCURSOR,
    RECT, PAINTSTRUCT, WNDCLASSEXA, TOOLINFOW, TRACKMOUSEEVENT,
    ClassStyle, WindowStyle, WindowMessage, ShowCmd, VirtualKey,
    StockObject, PenStyle, TrackMouseFlags, GetSysColorBrush,
    TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    GetModuleHandleA,
    RegisterClassExW, CreateWindowExW, DefWindowProcW,
    MoveWindow, ShowWindow, SetFocus, GetFocus, EnableWindow,
    GetClientRect, InvalidateRect,
    SendMessageW, TrackMouseEvent,
    BeginPaint, EndPaint, FillRect,
    CreateSolidBrush, CreatePen, SelectObject, DeleteObject,
    GetStockObject, RoundRect, Ellipse, DrawFocusRect,
    LoadCursorW,
}
use util {
    vkToString, modCtrl, modShift, modAlt, modMeta, noOpBool,
}
use core { windowHwnd, noOp1, noOpEnterLeave, noOpClickMod, noOpWheel, noOpKey }

class SwitchSlot {
    pub hwnd:        HWND
    pub winHandle:   i64
    pub tooltipHwnd: HWND
    pub mouseTracking: bool
    pub checked:     bool
    pub focused:     bool
    pub enabled:     bool
    pub onToggle: fn(bool)
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
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd        = hwnd
        this.winHandle   = winHandle
        this.tooltipHwnd = 0 as HWND
        this.mouseTracking = false
        this.checked     = false
        this.focused     = false
        this.enabled     = true
        this.onToggle    = noOpBool
        this.onFocus     = noOp1
        this.onBlur      = noOp1
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseDown  = noOpClickMod
        this.onMouseUp    = noOpClickMod
        this.onContextMenu = noOpClickMod
        this.onMouseWheel = noOpWheel
        this.onKeyDown = noOpKey
        this.onKeyUp   = noOpKey
    }
}


let SWITCHES = new Map<i64, SwitchSlot>()

// `[false]` until the `IlangGuiSwitch` window class is registered.
let SWITCH_CLASS_INITED: bool[] = [false]

// Colours (COLORREF = 0x00BBGGRR).
const TRACK_OFF:   u32 = 0x00BFBFBF as u32   // light grey
const TRACK_ON:    u32 = 0x00D47800 as u32   // #0078D4 in BGR
const TRACK_DISABLED: u32 = 0x00DDDDDD as u32
const THUMB_FILL:  u32 = 0x00FFFFFF as u32   // white
const THUMB_EDGE:  u32 = 0x00808080 as u32   // mid grey

@extern(C) {

fn paintSwitch(hwnd: HWND, slot: SwitchSlot, hdc: HDC) {
    let rc = RECT { left: 0, top: 0, right: 0, bottom: 0 }
    GetClientRect(hwnd, &rc)
    let w = rc.right - rc.left
    let h = rc.bottom - rc.top
    // Fill background with dialog grey so the rounded track shows
    // its rounded shape rather than sitting on raw black.
    FillRect(hdc, &rc, GetSysColorBrush(15 as i32))  // COLOR_BTNFACE

    // Track: rounded rect inset by 1px so the right/bottom edges
    // sit inside the client rect.
    let trackColor =
        if !slot.enabled { TRACK_DISABLED }
        elif slot.checked { TRACK_ON }
        else { TRACK_OFF }
    let trackBrush = CreateSolidBrush(trackColor)
    let oldBrush = SelectObject(hdc, trackBrush as HGDIOBJ)
    let oldPen = SelectObject(hdc, GetStockObject(StockObject.nullPen as i32))
    RoundRect(hdc, 0 as i32, 0 as i32, w, h, h, h)
    SelectObject(hdc, oldPen)
    SelectObject(hdc, oldBrush)
    DeleteObject(trackBrush as HGDIOBJ)

    // Thumb: circle inset 2px from the track edge, snapping to
    // the left when off and the right when on.
    let pad = 2 as i32
    let diam = h - 2 as i32 * pad
    let thumbX = if slot.checked { w - pad - diam } else { pad }
    let thumbY = pad
    let thumbBrush = CreateSolidBrush(THUMB_FILL)
    let thumbPen = CreatePen(PenStyle.solid as i32, 1, THUMB_EDGE)
    let oldB2 = SelectObject(hdc, thumbBrush as HGDIOBJ)
    let oldP2 = SelectObject(hdc, thumbPen as HGDIOBJ)
    Ellipse(hdc, thumbX, thumbY, thumbX + diam, thumbY + diam)
    SelectObject(hdc, oldP2)
    SelectObject(hdc, oldB2)
    DeleteObject(thumbBrush as HGDIOBJ)
    DeleteObject(thumbPen as HGDIOBJ)

    // Focus ring — standard dotted rect, inset 1px.
    if slot.focused {
        let fr = RECT {
            left: 1 as i32, top: 1 as i32,
            right: w - 1 as i32, bottom: h - 1 as i32,
        }
        DrawFocusRect(hdc, &fr)
    }
}

fn toggleAndNotify(hwnd: HWND, slot: SwitchSlot) {
    if !slot.enabled { return }
    slot.checked = !slot.checked
    InvalidateRect(hwnd, 0 as *const RECT, 0)
    let cb = slot.onToggle
    cb(slot.checked)
}

pub fn switchWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    let lo = (lParam & 0xFFFF) as f64
    let hi = ((lParam >> 16) & 0xFFFF) as f64
    match msg as WindowMessage {
        WindowMessage.paint {
            if let some(slot) = SWITCHES.get(hwnd) {
                let ps = new PAINTSTRUCT()
                let hdc = BeginPaint(hwnd as HWND, &ps)
                paintSwitch(hwnd as HWND, slot, hdc)
                EndPaint(hwnd as HWND, &ps)
            }
            return 0
        }
        WindowMessage.eraseBkgnd {
            // Painted in WM_PAINT.
            return 1
        }
        WindowMessage.setFocus {
            if let some(slot) = SWITCHES.get(hwnd) {
                slot.focused = true
                InvalidateRect(hwnd as HWND, 0 as *const RECT, 0)
                let cb = slot.onFocus
                cb()
            }
            return 0
        }
        WindowMessage.killFocus {
            if let some(slot) = SWITCHES.get(hwnd) {
                slot.focused = false
                InvalidateRect(hwnd as HWND, 0 as *const RECT, 0)
                let cb = slot.onBlur
                cb()
            }
            return 0
        }
        WindowMessage.mouseMove {
            if let some(slot) = SWITCHES.get(hwnd) {
                if !slot.mouseTracking {
                    let tme = TRACKMOUSEEVENT {
                        cbSize:      24 as u32,
                        dwFlags:     TrackMouseFlags.leave as u32,
                        hwndTrack:   hwnd as HWND,
                        dwHoverTime: 0 as u32,
                    }
                    TrackMouseEvent(&tme)
                    slot.mouseTracking = true
                    let cb = slot.onMouseEnter
                    cb(lo, hi)
                }
            }
            return 0
        }
        WindowMessage.mouseLeave {
            if let some(slot) = SWITCHES.get(hwnd) {
                slot.mouseTracking = false
                let cb = slot.onMouseLeave
                cb(lo, hi)
            }
            return 0
        }
        WindowMessage.lbuttondown {
            if let some(slot) = SWITCHES.get(hwnd) {
                SetFocus(slot.hwnd)
                let cb = slot.onMouseDown
                cb(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.lbuttonup {
            if let some(slot) = SWITCHES.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
                toggleAndNotify(hwnd as HWND, slot)
            }
            return 0
        }
        WindowMessage.mButtonDown {
            if let some(slot) = SWITCHES.get(hwnd) {
                let cb = slot.onMouseDown
                cb(lo, hi, 1, modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.mButtonUp {
            if let some(slot) = SWITCHES.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 1, modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.rButtonDown {
            if let some(slot) = SWITCHES.get(hwnd) {
                let ck = modCtrl()
                let sk = modShift()
                let ak = modAlt()
                let mk = modMeta()
                let cb = slot.onMouseDown
                cb(lo, hi, 2, ck, sk, ak, mk)
                let ctx = slot.onContextMenu
                ctx(lo, hi, 2, ck, sk, ak, mk)
            }
            return 0
        }
        WindowMessage.rButtonUp {
            if let some(slot) = SWITCHES.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 2, modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.mouseWheel {
            if let some(slot) = SWITCHES.get(hwnd) {
                let raw = ((wParam >> 16) & 0xFFFF) as i32
                let signed = if raw >= 0x8000 as i32 { raw - 0x10000 as i32 } else { raw }
                let dy = (signed as f64) / 120.0
                let cb = slot.onMouseWheel
                cb(0.0, dy, lo, hi,
                   modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.keydown {
            if let some(slot) = SWITCHES.get(hwnd) {
                let vk = wParam as u32 as VirtualKey
                let cb = slot.onKeyDown
                cb(vkToString(vk), modCtrl(), modShift(), modAlt(), modMeta())
                // Space / Enter activate the switch — Win32 button
                // convention.
                if vk == VirtualKey.space || vk == VirtualKey.enter {
                    toggleAndNotify(hwnd as HWND, slot)
                }
            }
            return 0
        }
        WindowMessage.keyup {
            if let some(slot) = SWITCHES.get(hwnd) {
                let vk = wParam as u32 as VirtualKey
                let cb = slot.onKeyUp
                cb(vkToString(vk), modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        _ {}
    }
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

fn registerSwitchClass() {
    if SWITCH_CLASS_INITED[0] { return }
    SWITCH_CLASS_INITED[0] = true

    let hInstance = GetModuleHandleA(0 as *const char)
    let classNameW = "IlangGuiSwitch".encodeUtf16()
    let classNamePtr: *const u16 = classNameW
    let wc = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   switchWndProc,
        cbClsExtra:    0,
        cbWndExtra:    0,
        hInstance:     hInstance as HINSTANCE,
        hIcon:         0 as HICON,
        hCursor:       LoadCursorW(0 as HINSTANCE, 32512 as *const u16),  // IDC_ARROW
        hbrBackground: 0 as HBRUSH,
        lpszMenuName:  0 as *const char,
        lpszClassName: (classNamePtr as i64) as *const char,
        hIconSm:       0 as HICON,
    }
    RegisterClassExW(&wc)
}

pub fn createNativeSwitch(
    windowHandle: i64,
    checked: bool,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    registerSwitchClass()

    let classW = "IlangGuiSwitch".encodeUtf16()
    let titleW = "".encodeUtf16()
    let visibleBits: u32 = if visible {
        WindowStyle.visible as u32
    } else {
        0 as u32
    }
    let disabledBits: u32 = if !enabled {
        WindowStyle.disabled as u32
    } else {
        0 as u32
    }
    // WS_TABSTOP = 0x00010000 — focus reachable via Tab.
    let style: u32 = (WindowStyle.child as u32) | visibleBits | disabledBits
                   | (0x00010000 as u32)
    let hInstance = GetModuleHandleA(0 as *const char)
    let hwnd = CreateWindowExW(
        0,
        classW,
        titleW,
        style,
        x as i32, y as i32, width as i32, height as i32,
        parent,
        0 as HMENU,
        hInstance as HINSTANCE,
        0 as *void
    )
    let handle = hwnd as i64
    let slot = new SwitchSlot(hwnd, windowHandle)
    slot.checked = checked
    slot.enabled = enabled
    SWITCHES[handle] = slot
    handle
}

pub fn setOnSwitchToggle(handle: i64, cb: fn(bool)) {
    if let some(slot) = SWITCHES.get(handle) { slot.onToggle = cb }
}
pub fn setSwitchChecked(handle: i64, checked: bool) {
    if let some(slot) = SWITCHES.get(handle) {
        if slot.checked != checked {
            slot.checked = checked
            InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
        }
    }
}
pub fn readSwitchChecked(handle: i64): bool {
    match SWITCHES.get(handle) {
        some(slot) { slot.checked }
        none       { false }
    }
}
pub fn setSwitchFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = SWITCHES.get(handle) {
        MoveWindow(slot.hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setSwitchEnabled(handle: i64, enabled: bool) {
    if let some(slot) = SWITCHES.get(handle) {
        slot.enabled = enabled
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
        InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
    }
}
pub fn setSwitchVisible(handle: i64, visible: bool) {
    if let some(slot) = SWITCHES.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}
pub fn setOnSwitchFocus(handle: i64, cb: fn()) {
    if let some(slot) = SWITCHES.get(handle) { slot.onFocus = cb }
}
pub fn setOnSwitchBlur(handle: i64, cb: fn()) {
    if let some(slot) = SWITCHES.get(handle) { slot.onBlur = cb }
}
pub fn setOnSwitchMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = SWITCHES.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnSwitchMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = SWITCHES.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnSwitchMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = SWITCHES.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnSwitchMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = SWITCHES.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnSwitchContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = SWITCHES.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnSwitchMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = SWITCHES.get(handle) { slot.onMouseWheel = cb }
}
pub fn setOnSwitchKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = SWITCHES.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnSwitchKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = SWITCHES.get(handle) { slot.onKeyUp = cb }
}
pub fn focusSwitch(handle: i64) {
    if let some(slot) = SWITCHES.get(handle) { SetFocus(slot.hwnd) }
}
pub fn blurSwitch(handle: i64) {
    if let some(slot) = SWITCHES.get(handle) {
        if (GetFocus() as i64) == (slot.hwnd as i64) {
            SetFocus(slot.winHandle as HWND)
        }
    }
}
pub fn setSwitchTooltip(handle: i64, text: string) {
    if let some(slot) = SWITCHES.get(handle) {
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
