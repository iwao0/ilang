/// Accordion — Win32 has no native disclosure container, so this
/// is a custom-drawn `IlangGuiAccordion` window that paints a
/// header strip (triangle + title) along the top and hosts a
/// child `IlangGuiAccordionContent` window underneath for the
/// user's widgets. Clicking the header toggles the content
/// child's visibility and re-sizes the container so the parent
/// layout sees the correct height.

use windows {
    HWND, HDC, HBRUSH, HPEN, HGDIOBJ, HFONT,
    HINSTANCE, HMENU, HICON, HCURSOR,
    POINT, RECT, PAINTSTRUCT, WNDCLASSEXA,
    ClassStyle, WindowStyle, WindowMessage, ShowCmd, VirtualKey,
    PenStyle, StockObject, BackgroundMode,
    GetModuleHandleA, GetParent,
    RegisterClassExW, CreateWindowExW, DefWindowProcW,
    MoveWindow, ShowWindow, SetFocus, GetFocus, EnableWindow,
    GetClientRect, InvalidateRect,
    SetWindowPos, SetWindowPosFlags,
    SendMessageW,
    BeginPaint, EndPaint, FillRect,
    CreateSolidBrush, CreatePen, SelectObject, DeleteObject,
    GetStockObject, GetSysColorBrush,
    DrawTextW,
    SetTextColor, SetBkMode,
    LoadCursorW,
}
use util { applyGuiFont, ensureGuiFont, ensureGdiplus }
use panel { registerPanelClass }
use gdiplus {
    GpGraphics, GpPen,
    SmoothingMode,
    GdipCreateFromHDC, GdipDeleteGraphics, GdipSetSmoothingMode,
    GdipCreatePen1, GdipDeletePen,
    GdipSetPenStartCap, GdipSetPenEndCap,
    GdipDrawLine,
}
use core { windowHwnd }

// DrawTextW flags (subset).
const DT_LEFT:       u32 = 0x00000000 as u32
const DT_VCENTER:    u32 = 0x00000004 as u32
const DT_SINGLELINE: u32 = 0x00000020 as u32
const DT_END_ELLIPSIS: u32 = 0x00008000 as u32

const HEADER_BG:   u32 = 0x00F0F0F0 as u32
const HEADER_FG:   u32 = 0x00000000 as u32
const BORDER:      u32 = 0x00B0B0B0 as u32

class AccordionSlot {
    pub host:     HWND
    pub content:  HWND
    pub winHandle: i64
    pub title:    string
    pub width:    f64
    pub headerH:  f64
    pub contentH: f64
    pub expanded: bool
    pub visible:  bool
    pub onToggle: fn(bool)
    pub init(host: HWND, content: HWND, winHandle: i64) {
        this.host = host
        this.content = content
        this.winHandle = winHandle
        this.title = ""
        this.width = 0.0
        this.headerH = 28.0
        this.contentH = 120.0
        this.expanded = false
        this.visible = true
        this.onToggle = accordionNoOp
    }
}

fn accordionNoOp(expanded: bool) {}

let ACCORDIONS = new Map<i64, AccordionSlot>()
let ACCORDION_CLASS_INITED: bool = false

@extern(C) {

fn paintAccordion(hwnd: HWND, slot: AccordionSlot, hdc: HDC) {
    let rc = RECT { left: 0, top: 0, right: 0, bottom: 0 }
    GetClientRect(hwnd, &rc)
    let w = rc.right - rc.left
    let hH = slot.headerH as i32

    let headerRc = RECT { left: 0 as i32, top: 0 as i32, right: w, bottom: hH }
    let hb = CreateSolidBrush(HEADER_BG)
    FillRect(hdc, &headerRc, hb)
    DeleteObject(hb as HGDIOBJ)

    // Chevron — `>` when collapsed, `v` when expanded. Two
    // anti-aliased line segments meeting at a vertex, drawn via
    // GDI+ so the strokes look like macOS's disclosure indicator
    // rather than a chunky filled triangle on Win32 GDI.
    let cx: f64 = 14.0
    let cy: f64 = (hH as f64) / 2.0
    // Chevron arm half-length (px). macOS uses a small indicator;
    // 4 px arms give a ~5×8 (collapsed) / 8×5 (expanded) chevron.
    let arm: f64 = 4.0

    ensureGdiplus()
    let g: *GpGraphics = 0 as *GpGraphics
    GdipCreateFromHDC(hdc, &g)
    if g as i64 != 0 as i64 {
        GdipSetSmoothingMode(g, SmoothingMode.antiAlias as i32)
        // 0xFF6B6B6B — neutral mid-grey matching the rest of the
        // gui-win32 chrome and roughly the macOS secondary label.
        let pen: *GpPen = 0 as *GpPen
        GdipCreatePen1(0xFF6B6B6B as u32, 1.5 as f32, 0 as i32, &pen)
        // LineCap.Round = 2 — same constant the spinner uses.
        GdipSetPenStartCap(pen, 2 as i32)
        GdipSetPenEndCap(pen, 2 as i32)
        if slot.expanded {
            // `v`: two arms descending from (cx ∓ arm, cy − arm/2)
            // to a vertex at (cx, cy + arm/2).
            let yTop:    f32 = (cy - arm * 0.5) as f32
            let yVertex: f32 = (cy + arm * 0.5) as f32
            GdipDrawLine(g, pen,
                (cx - arm) as f32, yTop, cx as f32, yVertex)
            GdipDrawLine(g, pen,
                (cx + arm) as f32, yTop, cx as f32, yVertex)
        } else {
            // `>`: two arms converging from (cx − arm/2, cy ∓ arm)
            // to a vertex at (cx + arm/2, cy).
            let xLeft:   f32 = (cx - arm * 0.5) as f32
            let xVertex: f32 = (cx + arm * 0.5) as f32
            GdipDrawLine(g, pen,
                xLeft, (cy - arm) as f32, xVertex, cy as f32)
            GdipDrawLine(g, pen,
                xLeft, (cy + arm) as f32, xVertex, cy as f32)
        }
        GdipDeletePen(pen)
        GdipDeleteGraphics(g)
    }

    // Title text — leave room for the triangle on the left.
    // Select the cached Segoe UI 9pt font; HDCs default to the
    // Win 3.x SYSTEM bitmap font, which looks blocky next to
    // the rest of the gui-win32 widgets that use WM_SETFONT.
    let textRc = RECT {
        left:   24 as i32, top: 0 as i32,
        right:  w - 4 as i32, bottom: hH,
    }
    SetBkMode(hdc, BackgroundMode.transparent as i32)
    SetTextColor(hdc, HEADER_FG)
    let guiFont = ensureGuiFont()
    let oldFont = if guiFont != 0 as i64 {
        SelectObject(hdc, guiFont as HFONT as HGDIOBJ)
    } else {
        0 as HGDIOBJ
    }
    let titleW = slot.title.encodeUtf16(false)
    DrawTextW(
        hdc, titleW, titleW.length as i32, &textRc,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS
    )
    if guiFont != 0 as i64 { SelectObject(hdc, oldFont) }

    // Bottom border of the header — only visible when expanded so
    // the seam between header and content is obvious.
    if slot.expanded {
        let bp = CreatePen(PenStyle.solid as i32, 1, BORDER)
        let oldBp = SelectObject(hdc, bp as HGDIOBJ)
        let oldBrush2 = SelectObject(hdc, GetStockObject(StockObject.nullBrush as i32))
        // Pixel-perfect bottom line via MoveToEx/LineTo would need
        // an extra import; FillRect a 1px strip instead.
        let line = RECT {
            left: 0 as i32, top: hH - 1 as i32,
            right: w, bottom: hH,
        }
        FillRect(hdc, &line, GetSysColorBrush(15 as i32))
        SelectObject(hdc, oldBp)
        SelectObject(hdc, oldBrush2)
        DeleteObject(bp as HGDIOBJ)
    }
}

fn applyExpansion(slot: AccordionSlot) {
    let totalH = if slot.expanded {
        slot.headerH + slot.contentH
    } else {
        slot.headerH
    } as i32
    // Resize the host without moving its top-left (caller owns x/y).
    let rc = RECT { left: 0, top: 0, right: 0, bottom: 0 }
    let parentRc = RECT { left: 0, top: 0, right: 0, bottom: 0 }
    GetClientRect(slot.host, &rc)
    // Read host's frame relative to parent via MapWindowPoints is
    // overkill — just use MoveWindow with the cached origin we get
    // from setAccordionFrame. SimpleSize-only here: SetWindowPos
    // would let us skip x/y; fall back to MoveWindow on the current
    // origin captured via GetWindowRect → ScreenToClient, but
    // setAccordionFrame is the canonical updater, so just resize the
    // host's height in place.
    // For now, the public `setAccordionFrame` re-issues MoveWindow
    // every time. Tell it via _stale and let the caller refresh.
    let cmd: i32 = if slot.expanded && slot.visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
    ShowWindow(slot.content, cmd)
}

pub fn accordionWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    let lo = (lParam & 0xFFFF) as i32
    let hi = ((lParam >> 16) & 0xFFFF) as i32
    match msg as WindowMessage {
        WindowMessage.paint {
            if let some(slot) = ACCORDIONS.get(hwnd) {
                let ps = new PAINTSTRUCT()
                let hdc = BeginPaint(hwnd as HWND, &ps)
                paintAccordion(hwnd as HWND, slot, hdc)
                EndPaint(hwnd as HWND, &ps)
            }
            return 0
        }
        WindowMessage.eraseBkgnd { return 1 }
        WindowMessage.command {
            // Forward to the top-level Window so notifications from
            // widgets the user nested inside the accordion (which
            // bubble up first through the content panel, then here)
            // reach gui-core's WM_COMMAND dispatch.
            let p = GetParent(hwnd as HWND)
            if (p as i64) != 0 as i64 {
                return SendMessageW(p, msg, wParam, lParam) as i64
            }
        }
        WindowMessage.notify {
            let p = GetParent(hwnd as HWND)
            if (p as i64) != 0 as i64 {
                return SendMessageW(p, msg, wParam, lParam) as i64
            }
        }
        WindowMessage.lbuttonup {
            if let some(slot) = ACCORDIONS.get(hwnd) {
                // Header strip is the only clickable area.
                if hi < slot.headerH as i32 {
                    slot.expanded = !slot.expanded
                    let cmd: i32 = if slot.expanded {
                        ShowCmd.show as i32
                    } else {
                        ShowCmd.hide as i32
                    }
                    ShowWindow(slot.content, cmd)
                    // Shrink / grow the host itself so children that
                    // used to sit over the content area don't leave
                    // stale pixels behind on collapse. SetWindowPos
                    // with NOMOVE | NOZORDER preserves the host's
                    // parent-relative origin (we don't cache it).
                    let rect = RECT { left: 0, top: 0, right: 0, bottom: 0 }
                    GetClientRect(slot.host, &rect)
                    let curW = rect.right - rect.left
                    let newH = if slot.expanded {
                        (slot.headerH + slot.contentH) as i32
                    } else {
                        slot.headerH as i32
                    }
                    SetWindowPos(
                        slot.host, 0 as HWND, 0, 0, curW, newH,
                        (SetWindowPosFlags.noMove as u32) |
                        (SetWindowPosFlags.noZOrder as u32)
                    )
                    InvalidateRect(slot.host, 0 as *const RECT, 1)
                    slot.onToggle(slot.expanded)
                }
            }
            return 0
        }
        _ {}
    }
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

fn registerAccordionClass() {
    if ACCORDION_CLASS_INITED { return }
    ACCORDION_CLASS_INITED = true

    let hInstance = GetModuleHandleA(0 as *const char)
    let classNameW = "IlangGuiAccordion".encodeUtf16()
    let classNamePtr: *const u16 = classNameW
    let wc = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   accordionWndProc,
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

pub fn createNativeAccordion(
    windowHandle: i64,
    title: string,
    x: f64, y: f64, width: f64,
    contentH: f64, headerH: f64,
    expanded: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    registerAccordionClass()

    let totalH = if expanded { headerH + contentH } else { headerH }
    let classW = "IlangGuiAccordion".encodeUtf16()
    let titleW = "".encodeUtf16()
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    let style: u32 = (WindowStyle.child as u32) | visibleBits
    let hInstance = GetModuleHandleA(0 as *const char)
    let host = CreateWindowExW(
        0, classW, titleW, style,
        x as i32, y as i32, width as i32, totalH as i32,
        parent, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
    )
    applyGuiFont(host)

    // Content area — IlangGuiPanel so that WM_COMMAND / WM_NOTIFY
    // from buttons / checkboxes / list controls hosted inside the
    // accordion bubble up to the top-level Window's wnd-proc. The
    // STATIC class swallowed those notifications, leaving cb.onToggle
    // and friends silently un-fired.
    registerPanelClass()
    let contentClassW = "IlangGuiPanel".encodeUtf16()
    let contentStyle: u32 = (WindowStyle.child as u32)
                          | (if expanded { WindowStyle.visible as u32 } else { 0 as u32 })
    let content = CreateWindowExW(
        0, contentClassW, "".encodeUtf16(), contentStyle,
        0 as i32, headerH as i32,
        width as i32, contentH as i32,
        host, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
    )

    let handle = host as i64
    let slot = new AccordionSlot(host, content, windowHandle)
    slot.title = title
    slot.width = width
    slot.headerH = headerH
    slot.contentH = contentH
    slot.expanded = expanded
    slot.visible = visible
    ACCORDIONS[handle] = slot
    handle
}

pub fn accordionContentHandle(handle: i64): i64 {
    match ACCORDIONS.get(handle) {
        some(slot) { slot.content as i64 }
        none       { -1 as i64 }
    }
}

pub fn setAccordionTitle(handle: i64, title: string) {
    if let some(slot) = ACCORDIONS.get(handle) {
        slot.title = title
        InvalidateRect(slot.host, 0 as *const RECT, 0)
    }
}

pub fn setAccordionExpanded(handle: i64, expanded: bool) {
    if let some(slot) = ACCORDIONS.get(handle) {
        if slot.expanded != expanded {
            slot.expanded = expanded
            let cmd: i32 = if expanded { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
            ShowWindow(slot.content, cmd)
            InvalidateRect(slot.host, 0 as *const RECT, 0)
        }
    }
}

pub fn readAccordionExpanded(handle: i64): bool {
    match ACCORDIONS.get(handle) {
        some(slot) { slot.expanded }
        none       { false }
    }
}

pub fn setAccordionFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = ACCORDIONS.get(handle) {
        slot.width = width
        MoveWindow(slot.host, x as i32, y as i32, width as i32, height as i32, 1)
        MoveWindow(slot.content, 0 as i32, slot.headerH as i32,
                   width as i32, slot.contentH as i32, 1)
    }
}

pub fn setAccordionContentHeight(handle: i64, h: f64) {
    if let some(slot) = ACCORDIONS.get(handle) {
        slot.contentH = h
        MoveWindow(slot.content, 0 as i32, slot.headerH as i32,
                   slot.width as i32, h as i32, 1)
    }
}

pub fn setAccordionHeaderHeight(handle: i64, h: f64) {
    if let some(slot) = ACCORDIONS.get(handle) {
        slot.headerH = h
        MoveWindow(slot.content, 0 as i32, h as i32,
                   slot.width as i32, slot.contentH as i32, 1)
        InvalidateRect(slot.host, 0 as *const RECT, 0)
    }
}

pub fn setAccordionVisible(handle: i64, visible: bool) {
    if let some(slot) = ACCORDIONS.get(handle) {
        slot.visible = visible
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.host, cmd)
    }
}

pub fn setAccordionTooltip(handle: i64, text: string) {
    // Tooltip deferred — host is a custom class; reusing the
    // tooltips_class32 plumbing the other widgets share would
    // pull in the TOOLINFOW dance.
}

pub fn setOnAccordionToggle(handle: i64, cb: fn(bool)) {
    if let some(slot) = ACCORDIONS.get(handle) { slot.onToggle = cb }
}

} // @extern(C)
