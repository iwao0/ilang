/// SegmentedControl. Win32 has no native segmented widget, so
/// this is a custom-drawn child window in its own `IlangGuiSegmented`
/// class. The control is one window that paints all segments
/// inside its client area; hit-testing maps the click x to a
/// segment index. Single-select tracking, exactly like the Cocoa
/// `selectOne` mode.

use windows {
    HWND, HDC, HBRUSH, HPEN, HFONT, HGDIOBJ,
    HINSTANCE, HMENU, HICON, HCURSOR,
    POINT, RECT, PAINTSTRUCT, WNDCLASSEXA, TOOLINFOW, TRACKMOUSEEVENT,
    ClassStyle, WindowStyle, WindowMessage, ShowCmd, VirtualKey,
    StockObject, PenStyle, BackgroundMode, TrackMouseFlags,
    TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    GetModuleHandleA,
    RegisterClassExW, CreateWindowExW, DefWindowProcW,
    MoveWindow, ShowWindow, SetFocus, GetFocus, EnableWindow,
    GetClientRect, InvalidateRect,
    SendMessageW, TrackMouseEvent,
    BeginPaint, EndPaint, FillRect,
    CreateSolidBrush, CreatePen, SelectObject, DeleteObject,
    GetStockObject, GetSysColorBrush, RoundRect, MoveToEx, LineTo,
    Rectangle, DrawFocusRect,
    LoadCursorW,
    SetTextColor, SetBkMode, DrawTextW,
}

// DrawTextW format flags (DT_* — bindings/windows doesn't expose
// them as named constants).
const DT_CENTER:       u32 = 0x00000001 as u32
const DT_VCENTER:      u32 = 0x00000004 as u32
const DT_SINGLELINE:   u32 = 0x00000020 as u32
const DT_END_ELLIPSIS: u32 = 0x00008000 as u32
use util {
    vkToString, modCtrl, modShift, modAlt, modMeta, applyGuiFont, ensureGuiFont,
}
use core { windowHwnd }

class SegmentedSlot {
    pub hwnd:        HWND
    pub winHandle:   i64
    pub tooltipHwnd: HWND
    pub mouseTracking: bool
    pub titles:      string[]
    pub selectedIndex: i32
    pub focused:     bool
    pub enabled:     bool
    pub onChange:    fn(i32)
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd        = hwnd
        this.winHandle   = winHandle
        this.tooltipHwnd = 0 as HWND
        this.mouseTracking = false
        this.titles      = []
        this.selectedIndex = -1 as i32
        this.focused     = false
        this.enabled     = true
        this.onChange    = segmentedNoOp
    }
}

fn segmentedNoOp(i: i32) {}

let SEGMENTEDS = new Map<i64, SegmentedSlot>()

// `[false]` until the `IlangGuiSegmented` window class is registered.
let SEGMENTED_CLASS_INITED: bool = false

// Colours (COLORREF = 0x00BBGGRR).
const BG_NORMAL:   u32 = 0x00F0F0F0 as u32
const BG_SELECTED: u32 = 0x00D47800 as u32   // #0078D4 in BGR
const BG_DISABLED: u32 = 0x00DDDDDD as u32
const FG_NORMAL:   u32 = 0x00000000 as u32
const FG_SELECTED: u32 = 0x00FFFFFF as u32
const FG_DISABLED: u32 = 0x00808080 as u32
const BORDER:      u32 = 0x00B0B0B0 as u32

@extern(C) {

fn segmentRect(slot: SegmentedSlot, totalW: i32, totalH: i32, i: i32): RECT {
    let n = slot.titles.length as i32
    if n <= 0 as i32 {
        return RECT { left: 0 as i32, top: 0 as i32, right: 0 as i32, bottom: 0 as i32 }
    }
    let segW = totalW / n
    let left = i * segW
    // Last segment soaks up the integer-divide remainder so the
    // right edge always lines up with the control's client edge.
    let right = if i == n - 1 as i32 { totalW } else { left + segW }
    RECT { left: left, top: 0 as i32, right: right, bottom: totalH }
}

fn segmentFromX(slot: SegmentedSlot, totalW: i32, x: i32): i32 {
    let n = slot.titles.length as i32
    if n <= 0 as i32 { return -1 as i32 }
    if x < 0 as i32 || x >= totalW { return -1 as i32 }
    let segW = totalW / n
    if segW <= 0 as i32 { return -1 as i32 }
    let idx = x / segW
    if idx >= n { n - 1 as i32 } else { idx }
}

fn paintSegmented(hwnd: HWND, slot: SegmentedSlot, hdc: HDC) {
    let rc = RECT { left: 0, top: 0, right: 0, bottom: 0 }
    GetClientRect(hwnd, &rc)
    let w = rc.right - rc.left
    let h = rc.bottom - rc.top

    // Background fill to dialog grey so any uncovered border pixel
    // blends in instead of showing raw black.
    FillRect(hdc, &rc, GetSysColorBrush(15 as i32))  // COLOR_BTNFACE

    SetBkMode(hdc, BackgroundMode.transparent as i32)

    // Install Segoe UI on the DC so DrawTextW renders segment
    // labels in the modern UI font instead of the default
    // SYSTEM_FONT bitmap face.
    let guiFont = ensureGuiFont() as HFONT
    let oldFont = SelectObject(hdc, guiFont as HGDIOBJ)

    let n = slot.titles.length as i32
    let i = 0 as i32
    while i < n {
        let segRect = segmentRect(slot, w, h, i)
        let isSel = i == slot.selectedIndex
        let bgColor =
            if !slot.enabled { BG_DISABLED }
            elif isSel       { BG_SELECTED }
            else             { BG_NORMAL }
        let bgBrush = CreateSolidBrush(bgColor)
        FillRect(hdc, &segRect, bgBrush)
        DeleteObject(bgBrush as HGDIOBJ)

        // Vertical separator between adjacent segments — drawn on
        // the right edge of every segment except the last.
        if i < n - 1 as i32 {
            let pen = CreatePen(PenStyle.solid as i32, 1, BORDER)
            let oldPen = SelectObject(hdc, pen as HGDIOBJ)
            MoveToEx(hdc, segRect.right - 1 as i32, 1 as i32, 0 as *POINT)
            LineTo(hdc, segRect.right - 1 as i32, h - 1 as i32)
            SelectObject(hdc, oldPen)
            DeleteObject(pen as HGDIOBJ)
        }

        let fg =
            if !slot.enabled { FG_DISABLED }
            elif isSel       { FG_SELECTED }
            else             { FG_NORMAL }
        SetTextColor(hdc, fg)
        let textW = slot.titles[i as i64].encodeUtf16(false)
        DrawTextW(
            hdc, textW, textW.length as i32, &segRect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS
        )
        i = i + 1 as i32
    }

    // Restore the DC's prior font now that we're done with text.
    SelectObject(hdc, oldFont)

    // Outer border + focus ring on the selected segment.
    let borderPen = CreatePen(PenStyle.solid as i32, 1, BORDER)
    let oldPen = SelectObject(hdc, borderPen as HGDIOBJ)
    let oldBrush = SelectObject(hdc, GetStockObject(StockObject.nullBrush as i32))
    Rectangle(hdc, 0 as i32, 0 as i32, w, h)
    SelectObject(hdc, oldPen)
    SelectObject(hdc, oldBrush)
    DeleteObject(borderPen as HGDIOBJ)

    if slot.focused && slot.selectedIndex >= 0 as i32 {
        let sr = segmentRect(slot, w, h, slot.selectedIndex)
        let fr = RECT {
            left: sr.left + 2 as i32, top: sr.top + 2 as i32,
            right: sr.right - 2 as i32, bottom: sr.bottom - 2 as i32,
        }
        DrawFocusRect(hdc, &fr)
    }
}

fn selectAndNotify(hwnd: HWND, slot: SegmentedSlot, idx: i32) {
    if !slot.enabled { return }
    if idx < 0 as i32 || idx >= slot.titles.length as i32 { return }
    if idx == slot.selectedIndex { return }
    slot.selectedIndex = idx
    InvalidateRect(hwnd, 0 as *const RECT, 0)
    slot.onChange(idx)
}

pub fn segmentedWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    let lo = (lParam & 0xFFFF) as i32
    match msg as WindowMessage {
        WindowMessage.paint {
            if let some(slot) = SEGMENTEDS.get(hwnd) {
                let ps = new PAINTSTRUCT()
                let hdc = BeginPaint(hwnd as HWND, &ps)
                paintSegmented(hwnd as HWND, slot, hdc)
                EndPaint(hwnd as HWND, &ps)
            }
            return 0
        }
        WindowMessage.eraseBkgnd {
            return 1
        }
        WindowMessage.setFocus {
            if let some(slot) = SEGMENTEDS.get(hwnd) {
                slot.focused = true
                InvalidateRect(hwnd as HWND, 0 as *const RECT, 0)
            }
            return 0
        }
        WindowMessage.killFocus {
            if let some(slot) = SEGMENTEDS.get(hwnd) {
                slot.focused = false
                InvalidateRect(hwnd as HWND, 0 as *const RECT, 0)
            }
            return 0
        }
        WindowMessage.mouseMove {
            if let some(slot) = SEGMENTEDS.get(hwnd) {
                if !slot.mouseTracking {
                    let tme = TRACKMOUSEEVENT {
                        cbSize:      24 as u32,
                        dwFlags:     TrackMouseFlags.leave as u32,
                        hwndTrack:   hwnd as HWND,
                        dwHoverTime: 0 as u32,
                    }
                    TrackMouseEvent(&tme)
                    slot.mouseTracking = true
                }
            }
            return 0
        }
        WindowMessage.mouseLeave {
            if let some(slot) = SEGMENTEDS.get(hwnd) {
                slot.mouseTracking = false
            }
            return 0
        }
        WindowMessage.lbuttondown {
            if let some(slot) = SEGMENTEDS.get(hwnd) {
                SetFocus(slot.hwnd)
            }
            return 0
        }
        WindowMessage.lbuttonup {
            if let some(slot) = SEGMENTEDS.get(hwnd) {
                let rc = RECT { left: 0, top: 0, right: 0, bottom: 0 }
                GetClientRect(hwnd as HWND, &rc)
                let w = rc.right - rc.left
                let idx = segmentFromX(slot, w, lo)
                selectAndNotify(hwnd as HWND, slot, idx)
            }
            return 0
        }
        WindowMessage.keydown {
            if let some(slot) = SEGMENTEDS.get(hwnd) {
                let vk = wParam as u32 as VirtualKey
                let n = slot.titles.length as i32
                if vk == VirtualKey.left && n > 0 as i32 {
                    let cur = slot.selectedIndex
                    let next = if cur <= 0 as i32 { 0 as i32 } else { cur - 1 as i32 }
                    selectAndNotify(hwnd as HWND, slot, next)
                } elif vk == VirtualKey.right && n > 0 as i32 {
                    let cur = slot.selectedIndex
                    let next = if cur < 0 as i32 { 0 as i32 }
                               elif cur >= n - 1 as i32 { n - 1 as i32 }
                               else { cur + 1 as i32 }
                    selectAndNotify(hwnd as HWND, slot, next)
                }
            }
            return 0
        }
        _ {}
    }
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

fn registerSegmentedClass() {
    if SEGMENTED_CLASS_INITED { return }
    SEGMENTED_CLASS_INITED = true

    let hInstance = GetModuleHandleA(0 as *const char)
    let classNameW = "IlangGuiSegmented".encodeUtf16()
    let classNamePtr: *const u16 = classNameW
    let wc = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   segmentedWndProc,
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

pub fn createNativeSegmented(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    registerSegmentedClass()

    let classW = "IlangGuiSegmented".encodeUtf16()
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
    // WS_TABSTOP = 0x00010000.
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
    applyGuiFont(hwnd)
    let handle = hwnd as i64
    let slot = new SegmentedSlot(hwnd, windowHandle)
    slot.enabled = enabled
    SEGMENTEDS[handle] = slot
    handle
}

pub fn segmentedAddSegment(handle: i64, title: string) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        slot.titles.push(title)
        InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
    }
}
pub fn segmentedClearSegments(handle: i64) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        slot.titles = []
        slot.selectedIndex = -1 as i32
        InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
    }
}
pub fn segmentedSetTitle(handle: i64, index: i32, title: string) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        let i = index as i64
        if i >= 0 as i64 && i < slot.titles.length {
            slot.titles[i] = title
            InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
        }
    }
}
pub fn segmentedGetTitle(handle: i64, index: i32): string {
    match SEGMENTEDS.get(handle) {
        some(slot) {
            let i = index as i64
            if i >= 0 as i64 && i < slot.titles.length {
                slot.titles[i]
            } else {
                ""
            }
        }
        none { "" }
    }
}
pub fn segmentedGetSelectedIndex(handle: i64): i32 {
    match SEGMENTEDS.get(handle) {
        some(slot) { slot.selectedIndex }
        none       { -1 as i32 }
    }
}
pub fn segmentedSetSelectedIndex(handle: i64, index: i32) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        if index >= 0 as i32 && (index as i64) < slot.titles.length {
            if slot.selectedIndex != index {
                slot.selectedIndex = index
                InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
            }
        }
    }
}
pub fn segmentedGetCount(handle: i64): i32 {
    match SEGMENTEDS.get(handle) {
        some(slot) { slot.titles.length as i32 }
        none       { 0 as i32 }
    }
}
pub fn setSegmentedFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        MoveWindow(slot.hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setSegmentedEnabled(handle: i64, enabled: bool) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        slot.enabled = enabled
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
        InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
    }
}
pub fn setSegmentedVisible(handle: i64, visible: bool) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}
pub fn setSegmentedTooltip(handle: i64, text: string) {
    if let some(slot) = SEGMENTEDS.get(handle) {
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
pub fn setOnSegmentedChange(handle: i64, cb: fn(i32)) {
    if let some(slot) = SEGMENTEDS.get(handle) { slot.onChange = cb }
}

} // @extern(C)
