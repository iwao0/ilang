/// ActivityIndicator — Win32 custom-drawn spinner.
///
/// Win32 has no native indeterminate spinner (the closest is
/// `msctls_progress32` with PBS_MARQUEE, but that draws a flat
/// bar rather than the circular dots users expect on every other
/// platform), so we host our own child window class
/// `IlangGuiActivityIndicator` and paint a 12-dot ring that
/// rotates on a `WM_TIMER`. GDI+ does the dot fills with
/// `SmoothingMode.antiAlias` so the ring stays smooth at small
/// sizes (16 / 24 / 32 px is the canonical use).

use windows {
    HWND, HDC, HBITMAP, HGDIOBJ, HINSTANCE, HMENU, HICON, HCURSOR, HBRUSH,
    RECT, PAINTSTRUCT, WNDCLASSEXA,
    ClassStyle, WindowStyle, WindowMessage,
    GetModuleHandleA, RegisterClassExW, CreateWindowExW, DefWindowProcW,
    MoveWindow, ShowWindow, EnableWindow,
    GetClientRect, InvalidateRect, BeginPaint, EndPaint, FillRect,
    GetSysColorBrush,
    CreateCompatibleDC, CreateCompatibleBitmap, SelectObject, DeleteObject, BitBlt,
    SetTimer, KillTimer,
    LoadCursorW,
    TOOLINFOW, TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    SendMessageW,
}
use core { windowHwnd }
use util { ensureGdiplus }
use gdiplus {
    GpGraphics, GpSolidFill, GpBrush, GpPen,
    SmoothingMode,
    GdipCreateFromHDC, GdipDeleteGraphics, GdipSetSmoothingMode,
    GdipCreateSolidFill, GdipDeleteBrush, GdipFillEllipse,
    GdipCreatePen1, GdipDeletePen, GdipSetPenStartCap, GdipSetPenEndCap,
    GdipDrawLine,
}
use std.math as math

// 8 radial bars (macOS-style spinner), advanced one slot per tick.
const SEG_COUNT: i32 = 8 as i32
const TIMER_INTERVAL_MS: u32 = 100 as u32
const TIMER_ID: u64 = 1 as u64
const TAU: f64 = 6.283185307179586
// LineCap.Round on the GDI+ side.
const LINE_CAP_ROUND: i32 = 2 as i32

class ActivityIndicatorSlot {
    pub hwnd:        HWND
    pub tooltipHwnd: HWND
    pub step:        i32
    pub running:     bool
    pub init(hwnd: HWND) {
        this.hwnd        = hwnd
        this.tooltipHwnd = 0 as HWND
        this.step        = 0 as i32
        this.running     = false
    }
}

let INDICATORS = new Map<i64, ActivityIndicatorSlot>()
let INDICATOR_CLASS_INITED: bool = false

@extern(C) {

fn paintIndicator(hwnd: HWND, slot: ActivityIndicatorSlot, hdc: HDC) {
    let rc = RECT { left: 0, top: 0, right: 0, bottom: 0 }
    GetClientRect(hwnd, &rc)
    let w = rc.right - rc.left
    let h = rc.bottom - rc.top
    if w <= 0 as i32 || h <= 0 as i32 { return }

    // Double-buffer: draw into an off-screen bitmap then BitBlt
    // once, otherwise the WM_PAINT clear+redraw cycle visibly
    // flickers on every timer tick.
    let memDC = CreateCompatibleDC(hdc)
    let memBmp = CreateCompatibleBitmap(hdc, w, h)
    let oldBmp = SelectObject(memDC, memBmp as HGDIOBJ)

    // Background: dialog grey so the ring blends into the parent.
    FillRect(memDC, &rc, GetSysColorBrush(15 as i32))  // COLOR_BTNFACE

    ensureGdiplus()
    let g: *GpGraphics = 0 as *GpGraphics
    let pg: **GpGraphics = &g
    GdipCreateFromHDC(memDC, pg)
    if g as i64 == 0 as i64 {
        SelectObject(memDC, oldBmp)
        DeleteObject(memBmp as HGDIOBJ)
        DeleteObject(memDC as HGDIOBJ)
        return
    }
    GdipSetSmoothingMode(g, SmoothingMode.antiAlias as i32)

    let cx: f64 = (w as f64) / 2.0
    let cy: f64 = (h as f64) / 2.0
    let shortSide: f64 = if w < h { w as f64 } else { h as f64 }
    // Bar geometry — radial line from inner to outer radius.
    // Tuned so the line caps don't crowd the centre at 16px; the
    // inner radius leaves a noticeable hub of empty space at the
    // middle (matches the macOS spinner's look).
    let inner:   f64 = shortSide * 0.26
    let outer:   f64 = shortSide * 0.46
    let barWidth: f64 = shortSide * 0.12
    let minBarWidth: f64 = 1.5
    let strokeW: f64 = if barWidth < minBarWidth { minBarWidth } else { barWidth }

    let i = 0 as i32
    while i < SEG_COUNT {
        // Bar `i` sits at angle (i / SEG_COUNT) of a full turn,
        // anchored at 12 o'clock (-π/2) so the active bar climbs
        // clockwise from the top — same direction as NSProgressIndicator.
        let ang  = (i as f64) * TAU / (SEG_COUNT as f64) - (TAU / 4.0)
        let cos_ = math.cos(ang)
        let sin_ = math.sin(ang)
        let x1   = cx + inner * cos_
        let y1   = cy + inner * sin_
        let x2   = cx + outer * cos_
        let y2   = cy + outer * sin_

        // Bar `step` is the brightest; trailing bars fade around
        // the ring (alpha drops the further you go from active).
        let rel: i32 = (slot.step - i + SEG_COUNT) % SEG_COUNT
        let t:   f64 = (rel as f64) / (SEG_COUNT as f64)
        let alpha8: i64 = (255.0 * (1.0 - 0.85 * t)) as i64
        let argb_i: i64 = (alpha8 * (0x1000000 as i64)) | (0x606060 as i64)
        let argb: u32 = argb_i as u32

        let pen: *GpPen = 0 as *GpPen
        let pPen: **GpPen = &pen
        GdipCreatePen1(argb, strokeW as f32, 0 as i32, pPen)
        GdipSetPenStartCap(pen, LINE_CAP_ROUND)
        GdipSetPenEndCap(pen, LINE_CAP_ROUND)
        GdipDrawLine(g, pen, x1 as f32, y1 as f32, x2 as f32, y2 as f32)
        GdipDeletePen(pen)
        i = i + 1 as i32
    }
    GdipDeleteGraphics(g)

    // SRCCOPY = 0x00CC0020 — single blit replaces the whole client
    // rect with the off-screen frame in one go, no flicker.
    BitBlt(hdc, 0 as i32, 0 as i32, w, h, memDC, 0 as i32, 0 as i32, 0x00CC0020 as u32)
    SelectObject(memDC, oldBmp)
    DeleteObject(memBmp as HGDIOBJ)
    DeleteObject(memDC as HGDIOBJ)
}

pub fn indicatorWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    match msg as WindowMessage {
        WindowMessage.paint {
            if let some(slot) = INDICATORS.get(hwnd) {
                let ps = new PAINTSTRUCT()
                let hdc = BeginPaint(hwnd as HWND, &ps)
                paintIndicator(hwnd as HWND, slot, hdc)
                EndPaint(hwnd as HWND, &ps)
            }
            return 0
        }
        WindowMessage.eraseBkgnd { return 1 }
        WindowMessage.timer {
            if let some(slot) = INDICATORS.get(hwnd) {
                slot.step = (slot.step + 1 as i32) % SEG_COUNT
                InvalidateRect(hwnd as HWND, 0 as *const RECT, 0)
            }
            return 0
        }
        _ {}
    }
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

fn registerIndicatorClass() {
    if INDICATOR_CLASS_INITED { return }
    INDICATOR_CLASS_INITED = true
    let hInstance = GetModuleHandleA(0 as *const char)
    let classNameW = "IlangGuiActivityIndicator".encodeUtf16()
    let classNamePtr: *const u16 = classNameW
    // IDC_ARROW = 32512.
    let arrow = LoadCursorW(0 as HINSTANCE, 32512 as *const u16)
    let wc = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   indicatorWndProc,
        cbClsExtra:    0,
        cbWndExtra:    0,
        hInstance:     hInstance as HINSTANCE,
        hIcon:         0 as HICON,
        hCursor:       arrow,
        hbrBackground: 16 as HBRUSH,   // COLOR_BTNFACE + 1
        lpszMenuName:  0 as *const char,
        lpszClassName: (classNamePtr as i64) as *const char,
        hIconSm:       0 as HICON,
    }
    RegisterClassExW(&wc)
}

pub fn createNativeActivityIndicator(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    registerIndicatorClass()

    let classW = "IlangGuiActivityIndicator".encodeUtf16()
    let titleW = "".encodeUtf16()
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    let style: u32 = (WindowStyle.child as u32) | visibleBits

    let hInstance = GetModuleHandleA(0 as *const char)
    let hwnd = CreateWindowExW(
        0,
        classW, titleW, style,
        x as i32, y as i32, width as i32, height as i32,
        parent, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
    )
    let handle = hwnd as i64
    INDICATORS[handle] = new ActivityIndicatorSlot(hwnd)
    handle
}

pub fn startActivityIndicator(handle: i64) {
    if let some(slot) = INDICATORS.get(handle) {
        if slot.running { return }
        slot.running = true
        SetTimer(slot.hwnd, TIMER_ID, TIMER_INTERVAL_MS, 0 as *void)
    }
}

pub fn stopActivityIndicator(handle: i64) {
    if let some(slot) = INDICATORS.get(handle) {
        if !slot.running { return }
        slot.running = false
        KillTimer(slot.hwnd, TIMER_ID)
        InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
    }
}

pub fn setActivityIndicatorFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = INDICATORS.get(handle) {
        MoveWindow(slot.hwnd, x as i32, y as i32, width as i32, height as i32, 1 as i32)
    }
}

pub fn setActivityIndicatorVisible(handle: i64, visible: bool) {
    if let some(slot) = INDICATORS.get(handle) {
        let cmd: i32 = if visible { 5 as i32 } else { 0 as i32 }   // SW_SHOW / SW_HIDE
        ShowWindow(slot.hwnd, cmd)
    }
}

pub fn setActivityIndicatorTooltip(handle: i64, text: string) {
    // Tooltips would require the same TOOLINFOW dance the other
    // custom-drawn widgets use. Skip for round 1 — the indicator
    // is rarely interactive enough to warrant one.
}

} // @extern(C)
