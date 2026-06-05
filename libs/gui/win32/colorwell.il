/// ColorWell — Win32 has no native swatch widget. This is a
/// custom-drawn `IlangGuiColorWell` child window that paints
/// the current colour as a rectangle and opens the standard
/// `ChooseColor` dialog when clicked. Alpha is ignored (Win32's
/// `ChooseColor` is RGB-only); the well's swatch always paints
/// the colour fully opaque.

use windows {
    HWND, HBRUSH, HPEN, HGDIOBJ, HFONT,
    HINSTANCE, HMENU, HICON, HCURSOR,
    RECT, PAINTSTRUCT, WNDCLASSEXA, CHOOSECOLORA, ChooseColorFlags,
    ClassStyle, WindowStyle, WindowMessage, ShowCmd, VirtualKey,
    PenStyle, StockObject,
    GetModuleHandleA,
    RegisterClassExW, CreateWindowExW, DefWindowProcW,
    MoveWindow, ShowWindow, SetFocus, GetFocus, EnableWindow,
    GetClientRect, InvalidateRect,
    BeginPaint, EndPaint, FillRect,
    CreateSolidBrush, CreatePen, SelectObject, DeleteObject,
    GetStockObject, GetSysColorBrush,
    Rectangle, DrawFocusRect,
    LoadCursorW,
    ChooseColorW,
}
use util {
    vkToString, modCtrl, modShift, modAlt, modMeta,
}
use core { windowHwnd }

class ColorWellSlot {
    pub hwnd:      HWND
    pub winHandle: i64
    pub color:     u32      // 0xAARRGGBB — kept in the public encoding
    pub focused:   bool
    pub enabled:   bool
    pub onChange:  fn(u32)
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd      = hwnd
        this.winHandle = winHandle
        this.color     = 0xFF000000 as u32
        this.focused   = false
        this.enabled   = true
        this.onChange  = colorWellNoOp
    }
}

fn colorWellNoOp(c: u32) {}

let COLORWELLS = new Map<i64, ColorWellSlot>()

// Persistent custom-colour palette shared across all ColorWells
// in the process — `ChooseColor` reads and writes to it.
let CUSTOM_COLORS: u32[] = [
    0xFFFFFF as u32, 0xFFFFFF as u32, 0xFFFFFF as u32, 0xFFFFFF as u32,
    0xFFFFFF as u32, 0xFFFFFF as u32, 0xFFFFFF as u32, 0xFFFFFF as u32,
    0xFFFFFF as u32, 0xFFFFFF as u32, 0xFFFFFF as u32, 0xFFFFFF as u32,
    0xFFFFFF as u32, 0xFFFFFF as u32, 0xFFFFFF as u32, 0xFFFFFF as u32,
]

let COLORWELL_CLASS_INITED: bool = false

// 0xAARRGGBB → 0x00BBGGRR (Win32 COLORREF).
fn argbToColorRef(argb: u32): u32 {
    let mask: u32 = 0xFF as u32
    let r = (argb >> 16) & mask
    let g = (argb >>  8) & mask
    let b =  argb        & mask
    r | (g << 8) | (b << 16)
}

// COLORREF (0x00BBGGRR) → opaque 0xFFRRGGBB.
fn colorRefToArgb(cr: u32): u32 {
    let mask: u32 = 0xFF as u32
    let b = (cr >> 16) & mask
    let g = (cr >>  8) & mask
    let r =  cr        & mask
    (0xFF as u32 << 24) | (r << 16) | (g << 8) | b
}

@extern(C) {

fn paintColorWell(hwnd: HWND, slot: ColorWellSlot, hdc: HDC) {
    let rc = RECT { left: 0, top: 0, right: 0, bottom: 0 }
    GetClientRect(hwnd, &rc)
    let w = rc.right - rc.left
    let h = rc.bottom - rc.top

    // Fill background to dialog grey so the swatch's inset border
    // sits on a neutral pad rather than raw black.
    FillRect(hdc, &rc, GetSysColorBrush(15 as i32))

    let inner = RECT { left: 2 as i32, top: 2 as i32,
                       right: w - 2 as i32, bottom: h - 2 as i32 }
    let cr = argbToColorRef(slot.color)
    let brush = CreateSolidBrush(cr)
    FillRect(hdc, &inner, brush)
    DeleteObject(brush as HGDIOBJ)

    // Outer 1px border.
    let pen = CreatePen(PenStyle.solid as i32, 1, 0x00808080 as u32)
    let oldPen = SelectObject(hdc, pen as HGDIOBJ)
    let oldBrush = SelectObject(hdc, GetStockObject(StockObject.nullBrush as i32))
    Rectangle(hdc, 0 as i32, 0 as i32, w, h)
    SelectObject(hdc, oldPen)
    SelectObject(hdc, oldBrush)
    DeleteObject(pen as HGDIOBJ)

    if slot.focused {
        let fr = RECT { left: 1 as i32, top: 1 as i32,
                        right: w - 1 as i32, bottom: h - 1 as i32 }
        DrawFocusRect(hdc, &fr)
    }
}

fn openPicker(slot: ColorWellSlot) {
    if !slot.enabled { return }
    let cc = CHOOSECOLORA {
        lStructSize:    72 as u32,
        hwndOwner:      slot.hwnd,
        hInstance:      0 as HINSTANCE,
        rgbResult:      argbToColorRef(slot.color),
        lpCustColors:   CUSTOM_COLORS as *u32,
        Flags:          (ChooseColorFlags.rgbInit as u32) | (0x00000002 as u32),
        // CC_FULLOPEN = 0x00000002 — open in expanded layout.
        lCustData:      0 as i64,
        lpfnHook:       0 as *void,
        lpTemplateName: 0 as *const char,
    }
    let p: *CHOOSECOLORA = &cc
    let ok = ChooseColorW(p)
    if ok != 0 as i32 {
        let newArgb = colorRefToArgb(cc.rgbResult)
        if newArgb != slot.color {
            slot.color = newArgb
            InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
            slot.onChange(newArgb)
        }
    }
}

pub fn colorWellWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    match msg as WindowMessage {
        WindowMessage.paint {
            if let some(slot) = COLORWELLS.get(hwnd) {
                let ps = new PAINTSTRUCT()
                let hdc = BeginPaint(hwnd as HWND, &ps)
                paintColorWell(hwnd as HWND, slot, hdc)
                EndPaint(hwnd as HWND, &ps)
            }
            return 0
        }
        WindowMessage.eraseBkgnd { return 1 }
        WindowMessage.setFocus {
            if let some(slot) = COLORWELLS.get(hwnd) {
                slot.focused = true
                InvalidateRect(hwnd as HWND, 0 as *const RECT, 0)
            }
            return 0
        }
        WindowMessage.killFocus {
            if let some(slot) = COLORWELLS.get(hwnd) {
                slot.focused = false
                InvalidateRect(hwnd as HWND, 0 as *const RECT, 0)
            }
            return 0
        }
        WindowMessage.lbuttondown {
            if let some(slot) = COLORWELLS.get(hwnd) {
                SetFocus(slot.hwnd)
            }
            return 0
        }
        WindowMessage.lbuttonup {
            if let some(slot) = COLORWELLS.get(hwnd) {
                openPicker(slot)
            }
            return 0
        }
        WindowMessage.keydown {
            if let some(slot) = COLORWELLS.get(hwnd) {
                let vk = wParam as u32 as VirtualKey
                if vk == VirtualKey.space || vk == VirtualKey.returnKey {
                    openPicker(slot)
                }
            }
            return 0
        }
        _ {}
    }
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

fn registerColorWellClass() {
    if COLORWELL_CLASS_INITED { return }
    COLORWELL_CLASS_INITED = true

    let hInstance = GetModuleHandleA(0 as *const char)
    let classNameW = "IlangGuiColorWell".encodeUtf16()
    let classNamePtr: *const u16 = classNameW
    let wc = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   colorWellWndProc,
        cbClsExtra:    0,
        cbWndExtra:    0,
        hInstance:     hInstance as HINSTANCE,
        hIcon:         0 as HICON,
        hCursor:       LoadCursorW(0 as HINSTANCE, 32649 as *const u16),  // IDC_HAND
        hbrBackground: 0 as HBRUSH,
        lpszMenuName:  0 as *const char,
        lpszClassName: (classNamePtr as i64) as *const char,
        hIconSm:       0 as HICON,
    }
    RegisterClassExW(&wc)
}

pub fn createNativeColorWell(
    windowHandle: i64,
    initial: u32,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    registerColorWellClass()

    let classW = "IlangGuiColorWell".encodeUtf16()
    let titleW = "".encodeUtf16()
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    let disabledBits: u32 = if !enabled { WindowStyle.disabled as u32 } else { 0 as u32 }
    let style: u32 = (WindowStyle.child as u32) | visibleBits | disabledBits
                   | (0x00010000 as u32)  // WS_TABSTOP
    let hInstance = GetModuleHandleA(0 as *const char)
    let hwnd = CreateWindowExW(
        0, classW, titleW, style,
        x as i32, y as i32, width as i32, height as i32,
        parent, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
    )
    let handle = hwnd as i64
    let slot = new ColorWellSlot(hwnd, windowHandle)
    slot.color = initial
    slot.enabled = enabled
    COLORWELLS[handle] = slot
    handle
}

pub fn readColorWellColor(handle: i64): u32 {
    match COLORWELLS.get(handle) {
        some(slot) { slot.color }
        none       { 0 as u32 }
    }
}
pub fn setColorWellColor(handle: i64, color: u32) {
    if let some(slot) = COLORWELLS.get(handle) {
        if slot.color != color {
            slot.color = color
            InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
        }
    }
}
pub fn setColorWellFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = COLORWELLS.get(handle) {
        MoveWindow(slot.hwnd, x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setColorWellEnabled(handle: i64, enabled: bool) {
    if let some(slot) = COLORWELLS.get(handle) {
        slot.enabled = enabled
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
        InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
    }
}
pub fn setColorWellVisible(handle: i64, visible: bool) {
    if let some(slot) = COLORWELLS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}
pub fn setColorWellTooltip(handle: i64, text: string) {
    // Tooltip support deferred.
}
pub fn setOnColorWellChange(handle: i64, cb: fn(u32)) {
    if let some(slot) = COLORWELLS.get(handle) { slot.onChange = cb }
}

} // @extern(C)
