/// 2D drawing surface — Win32 GDI backend. Each Canvas is a child
/// window of its own `IlangGuiCanvas` class that owns a persistent
/// memory-DC bitmap. The public drawing primitives rasterise into
/// that bitmap; the canvas `WM_PAINT` just `BitBlt`s it onto screen,
/// so the OS repaint after a resize / expose costs nothing in user
/// code.
///
/// Coordinate system: top-left origin, y-down (GDI MM_TEXT) — matches
/// the rest of gui-core.
///
/// Notes / limitations:
/// - GDI's `COLORREF` carries no alpha, so the argb colour's A byte
///   is dropped; fills are flat-opaque.
/// - Paths accumulate ilang-side (op + arg arrays) and replay into a
///   `BeginPath`/`EndPath` bracket on `fill` / `stroke`. Each of
///   `fill` / `stroke` consumes the GDI path, so a single path can't
///   be both filled and stroked without re-issuing it.
/// - `arc` maps to GDI `AngleArc`; in y-down device space its sweep
///   reads clockwise from 3 o'clock, matching the HTML Canvas default.

use std.math as math
use windows {
    HWND, HDC, HBITMAP, HBRUSH, HPEN, HGDIOBJ,
    HINSTANCE, HMENU, HICON, HCURSOR,
    POINT, RECT, PAINTSTRUCT, WNDCLASSEXA,
    ClassStyle, WindowStyle, WindowMessage, ShowCmd, VirtualKey,
    RasterOp, StockObject, PenStyle, BackgroundMode,
    GetModuleHandleA,
    RegisterClassExW, CreateWindowExW, DefWindowProcW,
    MoveWindow, ShowWindow, SetFocus,
    GetDC, ReleaseDC, BeginPaint, EndPaint, InvalidateRect, FillRect,
    CreateCompatibleDC, CreateCompatibleBitmap, SelectObject,
    DeleteObject, BitBlt, CreateSolidBrush, CreatePen, SetBkMode,
    GetStockObject, Rectangle, MoveToEx, LineTo, AngleArc, CloseFigure,
    BeginPath, EndPath, FillPath, StrokePath,
}
use util {
    vkToString, modCtrl, modShift, modAlt, modMeta,
}
use core { windowHwnd }

// 180 / π — radians (public API) → degrees (AngleArc).
const RAD_TO_DEG: f64 = 57.29577951308232

// Path op codes stored per canvas (see `CanvasSlot.pathOps`).
const OP_MOVE:  i64 = 0
const OP_LINE:  i64 = 1
const OP_ARC:   i64 = 2
const OP_CLOSE: i64 = 3

/// Per-canvas state. Keyed in `CANVASES` by the canvas HWND
/// (`hwnd as i64`) — same value gui-core sees as the public
/// `handle: i64`. Colours are stored as GDI `COLORREF` (0x00BBGGRR).
class CanvasSlot {
    pub hwnd:        HWND
    pub mdc:         HDC          // memory DC holding the bitmap
    pub bmp:         HBITMAP      // backing bitmap
    pub w:           f64
    pub h:           f64
    pub fillColor:   u32
    pub strokeColor: u32
    pub lineWidth:   f64
    // Accumulated path: parallel op-code + argument arrays.
    pub pathOps:     i64[]
    pub pathArgs:    f64[][]
    pub onKeyDown:    fn(string, bool, bool, bool, bool)
    pub onKeyUp:      fn(string, bool, bool, bool, bool)
    pub onMouseMove:  fn(f64, f64, u32, bool, bool, bool, bool)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)

    pub init(hwnd: HWND, mdc: HDC, bmp: HBITMAP, w: f64, h: f64) {
        this.hwnd        = hwnd
        this.mdc         = mdc
        this.bmp         = bmp
        this.w           = w
        this.h           = h
        this.fillColor   = 0 as u32
        this.strokeColor = 0 as u32
        this.lineWidth   = 1.0
        this.pathOps     = newOps()
        this.pathArgs    = newArgs()
        this.onKeyDown    = canvasNoOpKey
        this.onKeyUp      = canvasNoOpKey
        this.onMouseMove  = canvasNoOpMove
        this.onMouseDown  = canvasNoOpBtn
        this.onMouseUp    = canvasNoOpBtn
        this.onMouseWheel = canvasNoOpWheel
    }
}

fn canvasNoOpKey(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn canvasNoOpMove(x: f64, y: f64, b: u32, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn canvasNoOpBtn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn canvasNoOpWheel(dx: f64, dy: f64, x: f64, y: f64, ck: bool, sk: bool, ak: bool, mk: bool) {}

let CANVASES = new Map<i64, CanvasSlot>()

// `[false]` until the `IlangGuiCanvas` window class is registered.
let CANVAS_CLASS_INITED: bool[] = [false]

fn newOps(): i64[] { let a: i64[] = []; a }
fn newArgs(): f64[][] { let a: f64[][] = []; a }

/// argb (0xAARRGGBB) → GDI COLORREF (0x00BBGGRR). Alpha dropped.
fn argbToColorRef(argb: u32): u32 {
    let mask: u32 = 0xFF as u32
    let r = (argb >> 16) & mask
    let g = (argb >>  8) & mask
    let b =  argb        & mask
    r | (g << 8) | (b << 16)
}

@extern(C) {

/// Window procedure for every `IlangGuiCanvas` child. `WM_PAINT`
/// blits the backing bitmap; `WM_ERASEBKGND` is swallowed (we paint
/// every pixel ourselves, so the default erase would only flicker).
pub fn canvasWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    let lo = (lParam & 0xFFFF) as f64
    let hi = ((lParam >> 16) & 0xFFFF) as f64
    match msg as WindowMessage {
        WindowMessage.paint {
            if let some(slot) = CANVASES.get(hwnd) {
                let ps = new PAINTSTRUCT()
                let hdc = BeginPaint(hwnd as HWND, &ps)
                BitBlt(
                    hdc, 0, 0, slot.w as i32, slot.h as i32,
                    slot.mdc, 0, 0, RasterOp.srcCopy as u32
                )
                EndPaint(hwnd as HWND, &ps)
            }
            return 0
        }
        WindowMessage.eraseBkgnd {
            return 1
        }
        WindowMessage.keydown {
            if let some(slot) = CANVASES.get(hwnd) {
                let cb = slot.onKeyDown
                cb(vkToString(wParam as u32 as VirtualKey),
                   modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.keyup {
            if let some(slot) = CANVASES.get(hwnd) {
                let cb = slot.onKeyUp
                cb(vkToString(wParam as u32 as VirtualKey),
                   modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.mouseMove {
            if let some(slot) = CANVASES.get(hwnd) {
                // wParam holds the MK_* bitfield (bit0=L, bit1=R,
                // bit4=middle). Pack into the (L=bit0, R=bit1,
                // M=bit2) layout that gpuview exposes.
                let mk = wParam as u32
                let buttons =
                    (if (mk & 0x1 as u32) != 0 as u32 { 0x1 as u32 } else { 0 as u32 }) |
                    (if (mk & 0x2 as u32) != 0 as u32 { 0x2 as u32 } else { 0 as u32 }) |
                    (if (mk & 0x10 as u32) != 0 as u32 { 0x4 as u32 } else { 0 as u32 })
                let cb = slot.onMouseMove
                cb(lo, hi, buttons,
                   modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.lbuttondown {
            if let some(slot) = CANVASES.get(hwnd) {
                // Take focus so subsequent WM_KEYDOWN reaches us
                // — the canvas isn't a focusable class by default.
                SetFocus(slot.hwnd)
                let cb = slot.onMouseDown
                cb(lo, hi, 0 as i32, modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.lbuttonup {
            if let some(slot) = CANVASES.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 0 as i32, modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.rButtonDown {
            if let some(slot) = CANVASES.get(hwnd) {
                let cb = slot.onMouseDown
                cb(lo, hi, 2 as i32, modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.rButtonUp {
            if let some(slot) = CANVASES.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 2 as i32, modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.mButtonDown {
            if let some(slot) = CANVASES.get(hwnd) {
                let cb = slot.onMouseDown
                cb(lo, hi, 1 as i32, modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.mButtonUp {
            if let some(slot) = CANVASES.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 1 as i32, modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.mouseWheel {
            if let some(slot) = CANVASES.get(hwnd) {
                let raw = ((wParam >> 16) & 0xFFFF) as i32
                let signed = if raw >= 0x8000 as i32 { raw - 0x10000 as i32 } else { raw }
                let dy = (signed as f64) / 120.0
                let cb = slot.onMouseWheel
                cb(0.0, dy, lo, hi,
                   modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        _ {}
    }
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

fn registerCanvasClass() {
    if CANVAS_CLASS_INITED[0] { return }
    CANVAS_CLASS_INITED[0] = true

    let hInstance = GetModuleHandleA(0 as *const char)
    let classNameW = "IlangGuiCanvas".encodeUtf16()
    let classNamePtr: *const u16 = classNameW
    let wc = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   canvasWndProc,
        cbClsExtra:    0,
        cbWndExtra:    0,
        hInstance:     hInstance as HINSTANCE,
        hIcon:         0 as HICON,
        hCursor:       0 as HCURSOR,
        hbrBackground: 0 as HBRUSH,
        lpszMenuName:  0 as *const char,
        lpszClassName: (classNamePtr as i64) as *const char,
        hIconSm:       0 as HICON,
    }
    RegisterClassExW(&wc)
}

/// Fill the whole memory DC opaque white — initial / post-resize
/// blank state so the bitmap isn't undefined memory.
fn whiteFill(mdc: HDC, w: f64, h: f64) {
    SetBkMode(mdc, BackgroundMode.transparent as i32)
    let rc = RECT { left: 0, top: 0, right: w as i32, bottom: h as i32 }
    FillRect(mdc, &rc, GetStockObject(StockObject.whiteBrush as i32) as HBRUSH)
}

pub fn createNativeCanvas(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    registerCanvasClass()

    let classW = "IlangGuiCanvas".encodeUtf16()
    let titleW = "".encodeUtf16()
    let visibleBits: u32 = if visible {
        WindowStyle.visible as u32
    } else {
        0 as u32
    }
    let style: u32 = (WindowStyle.child as u32) | visibleBits
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

    let wdc = GetDC(hwnd)
    let bmp = CreateCompatibleBitmap(wdc, width as i32, height as i32)
    let mdc = CreateCompatibleDC(wdc)
    SelectObject(mdc, bmp as HGDIOBJ)
    ReleaseDC(hwnd, wdc)
    whiteFill(mdc, width, height)

    let handle = hwnd as i64
    CANVASES[handle] = new CanvasSlot(hwnd, mdc, bmp, width, height)
    handle
}

pub fn setCanvasFrame(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = CANVASES.get(handle) {
        MoveWindow(slot.hwnd, x as i32, y as i32, w as i32, h as i32, 1)
        // Pixel dimensions changed — reallocate the bitmap so drawing
        // coords line up with the blit. Previous contents are dropped.
        if w != slot.w || h != slot.h {
            let wdc = GetDC(slot.hwnd)
            let bmp = CreateCompatibleBitmap(wdc, w as i32, h as i32)
            ReleaseDC(slot.hwnd, wdc)
            let oldBmp = SelectObject(slot.mdc, bmp as HGDIOBJ)
            DeleteObject(oldBmp)
            slot.bmp = bmp
            slot.w = w
            slot.h = h
            whiteFill(slot.mdc, w, h)
            InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
        }
    }
}

pub fn setCanvasVisible(handle: i64, visible: bool) {
    if let some(slot) = CANVASES.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}

pub fn setCanvasFillColor(handle: i64, argb: u32) {
    if let some(slot) = CANVASES.get(handle) {
        slot.fillColor = argbToColorRef(argb)
    }
}

pub fn setCanvasStrokeColor(handle: i64, argb: u32) {
    if let some(slot) = CANVASES.get(handle) {
        slot.strokeColor = argbToColorRef(argb)
    }
}

pub fn setCanvasLineWidth(handle: i64, w: f64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.lineWidth = w
    }
}

pub fn canvasClear(handle: i64, argb: u32) {
    if let some(slot) = CANVASES.get(handle) {
        let brush = CreateSolidBrush(argbToColorRef(argb))
        let rc = RECT { left: 0, top: 0, right: slot.w as i32, bottom: slot.h as i32 }
        FillRect(slot.mdc, &rc, brush)
        DeleteObject(brush as HGDIOBJ)
        InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
    }
}

pub fn canvasFillRect(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = CANVASES.get(handle) {
        let brush = CreateSolidBrush(slot.fillColor)
        let rc = RECT {
            left: x as i32, top: y as i32,
            right: (x + w) as i32, bottom: (y + h) as i32,
        }
        FillRect(slot.mdc, &rc, brush)
        DeleteObject(brush as HGDIOBJ)
        InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
    }
}

pub fn canvasStrokeRect(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = CANVASES.get(handle) {
        let pen = CreatePen(PenStyle.solid as i32, slot.lineWidth as i32, slot.strokeColor)
        let oldPen = SelectObject(slot.mdc, pen as HGDIOBJ)
        let oldBrush = SelectObject(slot.mdc, GetStockObject(StockObject.nullBrush as i32))
        Rectangle(slot.mdc, x as i32, y as i32, (x + w) as i32, (y + h) as i32)
        SelectObject(slot.mdc, oldPen)
        SelectObject(slot.mdc, oldBrush)
        DeleteObject(pen as HGDIOBJ)
        InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
    }
}

pub fn canvasBeginPath(handle: i64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.pathOps = newOps()
        slot.pathArgs = newArgs()
    }
}

pub fn canvasMoveTo(handle: i64, x: f64, y: f64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.pathOps.push(OP_MOVE)
        slot.pathArgs.push([x, y])
    }
}

pub fn canvasLineTo(handle: i64, x: f64, y: f64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.pathOps.push(OP_LINE)
        slot.pathArgs.push([x, y])
    }
}

pub fn canvasArc(
    handle: i64,
    cx: f64, cy: f64, r: f64, startAngle: f64, endAngle: f64
) {
    if let some(slot) = CANVASES.get(handle) {
        slot.pathOps.push(OP_ARC)
        slot.pathArgs.push([cx, cy, r, startAngle, endAngle])
    }
}

pub fn canvasClosePath(handle: i64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.pathOps.push(OP_CLOSE)
        slot.pathArgs.push([0.0])
    }
}

/// Replay the accumulated path into the memory DC inside a
/// `BeginPath`/`EndPath` bracket. Caller selects the pen / brush and
/// issues `FillPath` / `StrokePath` afterwards.
fn replayPath(slot: CanvasSlot) {
    BeginPath(slot.mdc)
    let n = slot.pathOps.length
    let i = 0
    while i < n {
        let op = slot.pathOps[i]
        let a = slot.pathArgs[i]
        // Separate `if` statements (not an if/elif chain): each is a
        // standalone statement so the GDI calls' i32 results are
        // discarded; only one op code matches per iteration anyway.
        if op == OP_MOVE {
            MoveToEx(slot.mdc, a[0] as i32, a[1] as i32, 0 as *POINT)
        }
        if op == OP_LINE {
            LineTo(slot.mdc, a[0] as i32, a[1] as i32)
        }
        if op == OP_ARC {
            let cx = a[0]
            let cy = a[1]
            let r  = a[2]
            let s  = a[3]
            let e  = a[4]
            // AngleArc draws a line from the current point to the arc
            // start, so move there first to avoid a stray connector.
            let sx = cx + r * math.cos(s)
            let sy = cy + r * math.sin(s)
            MoveToEx(slot.mdc, sx as i32, sy as i32, 0 as *POINT)
            AngleArc(
                slot.mdc, cx as i32, cy as i32, r as u32,
                (s * RAD_TO_DEG) as f32, ((e - s) * RAD_TO_DEG) as f32
            )
        }
        if op == OP_CLOSE {
            CloseFigure(slot.mdc)
        }
        i = i + 1
    }
    EndPath(slot.mdc)
}

pub fn canvasFill(handle: i64) {
    if let some(slot) = CANVASES.get(handle) {
        let brush = CreateSolidBrush(slot.fillColor)
        let oldBrush = SelectObject(slot.mdc, brush as HGDIOBJ)
        replayPath(slot)
        FillPath(slot.mdc)
        SelectObject(slot.mdc, oldBrush)
        DeleteObject(brush as HGDIOBJ)
        InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
    }
}

pub fn canvasStroke(handle: i64) {
    if let some(slot) = CANVASES.get(handle) {
        let pen = CreatePen(PenStyle.solid as i32, slot.lineWidth as i32, slot.strokeColor)
        let oldPen = SelectObject(slot.mdc, pen as HGDIOBJ)
        replayPath(slot)
        StrokePath(slot.mdc)
        SelectObject(slot.mdc, oldPen)
        DeleteObject(pen as HGDIOBJ)
        InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
    }
}

pub fn setOnCanvasKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = CANVASES.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnCanvasKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = CANVASES.get(handle) { slot.onKeyUp = cb }
}
pub fn setOnCanvasMouseMove(
    handle: i64, cb: fn(f64, f64, u32, bool, bool, bool, bool)
) {
    if let some(slot) = CANVASES.get(handle) { slot.onMouseMove = cb }
}
pub fn setOnCanvasMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = CANVASES.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnCanvasMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = CANVASES.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnCanvasMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = CANVASES.get(handle) { slot.onMouseWheel = cb }
}

} // @extern(C)
