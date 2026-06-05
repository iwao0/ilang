/// Image display widget — GDI+ paints the user-supplied PNG /
/// JPEG / BMP / etc. into a child window. Each ImageView owns
/// its own window class (`IlangGuiImageView`), a GpImage handle
/// for the current source, and a subclass proc for mouse events.

use std.ffi { writeU64 }
use windows {
    HWND, HINSTANCE, HMENU, RECT, HICON, HCURSOR, HBRUSH,
    HDC, PAINTSTRUCT, WNDCLASSEXA, ClassStyle,
    WindowStyle, WindowMessage, ShowCmd, TrackMouseFlags, TRACKMOUSEEVENT,
    TOOLINFOW, TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    GetModuleHandleA,
    RegisterClassExW, CreateWindowExW, DefWindowProcW,
    EnableWindow, MoveWindow, ShowWindow,
    BeginPaint, EndPaint, GetClientRect, InvalidateRect, FillRect,
    GetSysColorBrush, SysColor,
    SendMessageW, TrackMouseEvent,
}
use gdiplus {
    GpImage, GpGraphics,
    GdipLoadImageFromFile, GdipDisposeImage,
    GdipCreateFromHDC, GdipDeleteGraphics,
    GdipDrawImageRect,
    GpStatus,
}
use util { ensureGdiplus }
use util { modCtrl, modShift, modAlt, modMeta }
use core { windowHwnd }

class ImageViewSlot {
    pub hwnd:     HWND
    pub winHandle: i64
    pub tooltipHwnd: HWND
    pub image:    i64
    pub onMouseEnter:  fn(f64, f64)
    pub onMouseLeave:  fn(f64, f64)
    pub onMouseDown:   fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:     fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel:  fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub mouseTracking: bool
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd = hwnd
        this.winHandle = winHandle
        this.tooltipHwnd = 0 as HWND
        this.image = 0 as i64
        this.onMouseEnter  = ivNoOpEnterLeave
        this.onMouseLeave  = ivNoOpEnterLeave
        this.onMouseDown   = ivNoOpClickMod
        this.onMouseUp     = ivNoOpClickMod
        this.onContextMenu = ivNoOpClickMod
        this.onMouseWheel  = ivNoOpWheel
        this.mouseTracking = false
    }
}

fn ivNoOpEnterLeave(x: f64, y: f64) {}
fn ivNoOpClickMod(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn ivNoOpWheel(dx: f64, dy: f64, x: f64, y: f64, ck: bool, sk: bool, ak: bool, mk: bool) {}

let IMAGE_VIEWS = new Map<i64, ImageViewSlot>()
let CLASS_INITED: bool[] = [false]
@extern(C) {

pub fn imageViewWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    let lo = (lParam & 0xFFFF) as f64
    let hi = ((lParam >> 16) & 0xFFFF) as f64
    match msg as WindowMessage {
        WindowMessage.paint {
            if let some(slot) = IMAGE_VIEWS.get(hwnd) {
                let ps = new PAINTSTRUCT()
                let hdc = BeginPaint(hwnd as HWND, &ps)
                let rc = new RECT()
                GetClientRect(hwnd as HWND, &rc)
                // Background — paint window colour first so an
                // image smaller than the view shows through.
                FillRect(hdc, &rc, GetSysColorBrush(SysColor.window as i32))
                if slot.image != 0 as i64 {
                    let g: *GpGraphics = 0 as *GpGraphics
                    let pg: **GpGraphics = &g
                    GdipCreateFromHDC(hdc, pg)
                    if g as i64 != 0 as i64 {
                        // Stretch image to fill the view's client
                        // rect — the default ScaleToFit for every
                        // platform's MVP.
                        GdipDrawImageRect(
                            g, slot.image as *GpImage,
                            0.0 as f32, 0.0 as f32,
                            (rc.right - rc.left) as f32,
                            (rc.bottom - rc.top) as f32
                        )
                        GdipDeleteGraphics(g)
                    }
                }
                EndPaint(hwnd as HWND, &ps)
            }
            return 0
        }
        WindowMessage.eraseBkgnd { return 1 }
        WindowMessage.mouseMove {
            if let some(slot) = IMAGE_VIEWS.get(hwnd) {
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
            if let some(slot) = IMAGE_VIEWS.get(hwnd) {
                slot.mouseTracking = false
                let cb = slot.onMouseLeave
                cb(lo, hi)
            }
            return 0
        }
        WindowMessage.lbuttondown {
            if let some(slot) = IMAGE_VIEWS.get(hwnd) {
                let cb = slot.onMouseDown
                cb(lo, hi, 0 as i32, modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.lbuttonup {
            if let some(slot) = IMAGE_VIEWS.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 0 as i32, modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.rButtonDown {
            if let some(slot) = IMAGE_VIEWS.get(hwnd) {
                let ck = modCtrl()
                let sk = modShift()
                let ak = modAlt()
                let mk = modMeta()
                let cb = slot.onMouseDown
                cb(lo, hi, 2 as i32, ck, sk, ak, mk)
                let ctx = slot.onContextMenu
                ctx(lo, hi, 2 as i32, ck, sk, ak, mk)
            }
            return 0
        }
        WindowMessage.rButtonUp {
            if let some(slot) = IMAGE_VIEWS.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 2 as i32, modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.mButtonDown {
            if let some(slot) = IMAGE_VIEWS.get(hwnd) {
                let cb = slot.onMouseDown
                cb(lo, hi, 1 as i32, modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.mButtonUp {
            if let some(slot) = IMAGE_VIEWS.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 1 as i32, modCtrl(), modShift(), modAlt(), modMeta())
            }
            return 0
        }
        WindowMessage.mouseWheel {
            if let some(slot) = IMAGE_VIEWS.get(hwnd) {
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

fn registerImageViewClass() {
    if CLASS_INITED[0] { return }
    CLASS_INITED[0] = true
    let hInstance = GetModuleHandleA(0 as *const char)
    let classNameW = "IlangGuiImageView".encodeUtf16()
    let classNamePtr: *const u16 = classNameW
    let wc = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   imageViewWndProc,
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

pub fn createNativeImageView(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    ensureGdiplus()
    registerImageViewClass()

    let classW = "IlangGuiImageView".encodeUtf16()
    let titleW = "".encodeUtf16()
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    let disabledBits: u32 = if !enabled { WindowStyle.disabled as u32 } else { 0 as u32 }
    let style: u32 = (WindowStyle.child as u32) | visibleBits | disabledBits
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
    IMAGE_VIEWS[handle] = new ImageViewSlot(hwnd, windowHandle)
    handle
}

pub fn setImageViewSrc(handle: i64, path: string) {
    if let some(slot) = IMAGE_VIEWS.get(handle) {
        if slot.image != 0 as i64 {
            GdipDisposeImage(slot.image as *GpImage)
            slot.image = 0 as i64
        }
        if path != "" {
            let pathW = path.encodeUtf16()
            let img: *GpImage = 0 as *GpImage
            let pp: **GpImage = &img
            GdipLoadImageFromFile(pathW, pp)
            slot.image = img as i64
        }
        InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
    }
}

pub fn setImageViewFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = IMAGE_VIEWS.get(handle) {
        MoveWindow(slot.hwnd, x as i32, y as i32, width as i32, height as i32, 1)
    }
}

pub fn setImageViewEnabled(handle: i64, enabled: bool) {
    if let some(slot) = IMAGE_VIEWS.get(handle) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
    }
}
pub fn setImageViewVisible(handle: i64, visible: bool) {
    if let some(slot) = IMAGE_VIEWS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}

pub fn setOnImageViewMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnImageViewMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnImageViewMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnImageViewMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnImageViewContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnImageViewMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onMouseWheel = cb }
}

pub fn setImageViewTooltip(handle: i64, text: string) {
    if let some(slot) = IMAGE_VIEWS.get(handle) {
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
