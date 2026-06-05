/// Scrollable container — Win32 backend. Each ScrollView is a
/// child `IlangGuiScrollView` HWND with WS_HSCROLL / WS_VSCROLL
/// styles; inside it sits an `IlangGuiScrollViewInner` HWND
/// sized to the scrollable content rectangle. Child widgets
/// parent under the inner pane (its HWND is what
/// `scrollViewDocumentHandle` returns), so they live in the
/// content space, not the visible viewport.
///
/// WM_HSCROLL / WM_VSCROLL / WM_MOUSEWHEEL on the outer window
/// drive the scroll positions; we update them in `slot.scrollX`
/// / `slot.scrollY` and `MoveWindow` the inner pane to
/// `(-scrollX, -scrollY)` so the visible content shifts.
/// WM_SIZE re-runs the scroll-info plumbing so the thumb size
/// stays in sync with the viewport.

use windows {
    HWND, HMENU, HINSTANCE, HBRUSH, HICON, HCURSOR,
    POINT, RECT, WNDCLASSEXA, SCROLLINFO,
    ClassStyle, WindowStyle, WindowMessage, ShowCmd,
    GetModuleHandleA, RegisterClassExW, CreateWindowExW,
    DefWindowProcW, MoveWindow, ShowWindow, EnableWindow,
    SetScrollInfo, GetScrollInfo, ShowScrollBar, GetClientRect,
}
use core { windowHwnd }

// SB_HORZ = 0, SB_VERT = 1 — `nBar` arg to scroll-info calls.
const SB_HORZ: i32 = 0
const SB_VERT: i32 = 1

// SCROLLINFO.fMask bits.
const SIF_RANGE: u32 = 0x0001 as u32
const SIF_PAGE:  u32 = 0x0002 as u32
const SIF_POS:   u32 = 0x0004 as u32
const SIF_ALL:   u32 = 0x0017 as u32

// SB_* — WM_HSCROLL / WM_VSCROLL notification codes (LOWORD of wParam).
const SB_LINEUP:        i32 = 0
const SB_LINEDOWN:      i32 = 1
const SB_PAGEUP:        i32 = 2
const SB_PAGEDOWN:      i32 = 3
const SB_THUMBPOSITION: i32 = 4
const SB_THUMBTRACK:    i32 = 5
const SB_TOP:           i32 = 6
const SB_BOTTOM:        i32 = 7

class ScrollViewSlot {
    pub outer:    HWND
    pub inner:    HWND
    pub contentW: i32
    pub contentH: i32
    pub scrollX:  i32
    pub scrollY:  i32
    pub init(outer: HWND, inner: HWND, w: i32, h: i32) {
        this.outer    = outer
        this.inner    = inner
        this.contentW = w
        this.contentH = h
        this.scrollX  = 0 as i32
        this.scrollY  = 0 as i32
    }
}

let SCROLL_VIEWS = new Map<i64, ScrollViewSlot>()
let SCROLL_VIEW_CLASS_INITED: bool[] = [false]

@extern(C) {

/// Push the current `(contentW, contentH, scrollX, scrollY)` to
/// the outer HWND's two scroll bars so the thumbs reflect the
/// viewport's position inside the content rectangle.
fn updateScrollInfo(slot: ScrollViewSlot) {
    let rc = RECT { left: 0 as i32, top: 0 as i32, right: 0 as i32, bottom: 0 as i32 }
    GetClientRect(slot.outer, &rc)
    let viewW = rc.right - rc.left
    let viewH = rc.bottom - rc.top

    let si = SCROLLINFO {
        cbSize:    28 as u32,
        fMask:     SIF_RANGE | SIF_PAGE | SIF_POS,
        nMin:      0 as i32,
        nMax:      slot.contentW,
        nPage:     viewW as u32,
        nPos:      slot.scrollX,
        nTrackPos: 0 as i32,
    }
    SetScrollInfo(slot.outer, SB_HORZ, &si, 1 as i32)

    si.nMax  = slot.contentH
    si.nPage = viewH as u32
    si.nPos  = slot.scrollY
    SetScrollInfo(slot.outer, SB_VERT, &si, 1 as i32)
}

/// Pin `(scrollX, scrollY)` inside `[0, content − viewport]` so
/// the user can't scroll off the end of the content rectangle.
fn clampScroll(slot: ScrollViewSlot) {
    let rc = RECT { left: 0 as i32, top: 0 as i32, right: 0 as i32, bottom: 0 as i32 }
    GetClientRect(slot.outer, &rc)
    let viewW = rc.right - rc.left
    let viewH = rc.bottom - rc.top

    let maxX = if slot.contentW > viewW { slot.contentW - viewW } else { 0 as i32 }
    let maxY = if slot.contentH > viewH { slot.contentH - viewH } else { 0 as i32 }
    if slot.scrollX < 0 as i32 { slot.scrollX = 0 as i32 }
    if slot.scrollX > maxX     { slot.scrollX = maxX }
    if slot.scrollY < 0 as i32 { slot.scrollY = 0 as i32 }
    if slot.scrollY > maxY     { slot.scrollY = maxY }
}

/// Move the inner pane to reflect the current scroll offsets,
/// then republish the scroll-info so the thumbs follow.
fn applyScroll(slot: ScrollViewSlot) {
    clampScroll(slot)
    MoveWindow(
        slot.inner,
        0 as i32 - slot.scrollX, 0 as i32 - slot.scrollY,
        slot.contentW, slot.contentH, 1 as i32
    )
    updateScrollInfo(slot)
}

/// Translate a WM_HSCROLL / WM_VSCROLL notification into a new
/// `scrollX` or `scrollY` value, then `applyScroll`. The
/// `vertical` flag picks which axis we're updating; `bar` is the
/// `SB_HORZ` / `SB_VERT` value the next `GetScrollInfo` reads.
fn handleScroll(slot: ScrollViewSlot, bar: i32, code: i32, vertical: bool) {
    let info = SCROLLINFO {
        cbSize:    28 as u32,
        fMask:     SIF_ALL,
        nMin:      0 as i32,
        nMax:      0 as i32,
        nPage:     0 as u32,
        nPos:      0 as i32,
        nTrackPos: 0 as i32,
    }
    GetScrollInfo(slot.outer, bar, &info)

    let lineStep = 20 as i32
    let newPos: i32 = info.nPos
    if code == SB_LINEUP        { newPos = newPos - lineStep }
    if code == SB_LINEDOWN      { newPos = newPos + lineStep }
    if code == SB_PAGEUP        { newPos = newPos - info.nPage as i32 }
    if code == SB_PAGEDOWN      { newPos = newPos + info.nPage as i32 }
    if code == SB_THUMBPOSITION { newPos = info.nTrackPos }
    if code == SB_THUMBTRACK    { newPos = info.nTrackPos }
    if code == SB_TOP           { newPos = info.nMin }
    if code == SB_BOTTOM        { newPos = info.nMax - info.nPage as i32 }

    if vertical {
        slot.scrollY = newPos
    } else {
        slot.scrollX = newPos
    }
    applyScroll(slot)
}

pub fn scrollViewWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    match msg as WindowMessage {
        WindowMessage.vScroll {
            if let some(slot) = SCROLL_VIEWS.get(hwnd) {
                let code = (wParam & 0xFFFF) as i32
                handleScroll(slot, SB_VERT, code, true)
            }
            return 0
        }
        WindowMessage.hScroll {
            if let some(slot) = SCROLL_VIEWS.get(hwnd) {
                let code = (wParam & 0xFFFF) as i32
                handleScroll(slot, SB_HORZ, code, false)
            }
            return 0
        }
        WindowMessage.mouseWheel {
            if let some(slot) = SCROLL_VIEWS.get(hwnd) {
                let raw    = ((wParam >> 16) & 0xFFFF) as i32
                let signed = if raw >= 0x8000 as i32 { raw - 0x10000 as i32 } else { raw }
                // Scrolling up (positive wheel delta) means revealing
                // earlier content — decrement scrollY.
                let step = (signed / 120 as i32) * 30 as i32
                slot.scrollY = slot.scrollY - step
                applyScroll(slot)
            }
            return 0
        }
        WindowMessage.size {
            if let some(slot) = SCROLL_VIEWS.get(hwnd) {
                applyScroll(slot)
            }
            return 0
        }
        _ {}
    }
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

fn defWndProcTrampoline(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

fn registerScrollViewClass() {
    if SCROLL_VIEW_CLASS_INITED[0] { return }
    SCROLL_VIEW_CLASS_INITED[0] = true

    let hInstance = GetModuleHandleA(0 as *const char)

    // Outer class: hosts the scroll bars and runs our WndProc.
    let outerNameW = "IlangGuiScrollView".encodeUtf16()
    let outerNamePtr: *const u16 = outerNameW
    let wcOuter = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   scrollViewWndProc,
        cbClsExtra:    0,
        cbWndExtra:    0,
        hInstance:     hInstance as HINSTANCE,
        hIcon:         0 as HICON,
        hCursor:       0 as HCURSOR,
        hbrBackground: 6 as HBRUSH,            // COLOR_WINDOW + 1
        lpszMenuName:  0 as *const char,
        lpszClassName: (outerNamePtr as i64) as *const char,
        hIconSm:       0 as HICON,
    }
    RegisterClassExW(&wcOuter)

    // Inner class: a plain content pane. DefWindowProcW handles
    // everything; children draw themselves on top.
    let innerNameW = "IlangGuiScrollViewInner".encodeUtf16()
    let innerNamePtr: *const u16 = innerNameW
    let wcInner = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   defWndProcTrampoline,
        cbClsExtra:    0,
        cbWndExtra:    0,
        hInstance:     hInstance as HINSTANCE,
        hIcon:         0 as HICON,
        hCursor:       0 as HCURSOR,
        hbrBackground: 6 as HBRUSH,
        lpszMenuName:  0 as *const char,
        lpszClassName: (innerNamePtr as i64) as *const char,
        hIconSm:       0 as HICON,
    }
    RegisterClassExW(&wcInner)
}

pub fn createNativeScrollView(
    parentHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool,
    showHScroll: bool, showVScroll: bool
): i64 {
    let parent = windowHwnd(parentHandle)
    if parent as i64 == 0 as i64 { return -1 }
    registerScrollViewClass()

    let visibleBits:  u32 = if visible      { WindowStyle.visible  as u32 } else { 0 as u32 }
    let disabledBits: u32 = if !enabled     { WindowStyle.disabled as u32 } else { 0 as u32 }
    let hScrollBits:  u32 = if showHScroll  { WindowStyle.hScroll  as u32 } else { 0 as u32 }
    let vScrollBits:  u32 = if showVScroll  { WindowStyle.vScroll  as u32 } else { 0 as u32 }
    let style: u32 = (WindowStyle.child as u32)
                   | visibleBits | disabledBits | hScrollBits | vScrollBits

    let hInstance = GetModuleHandleA(0 as *const char)
    let outerClassW = "IlangGuiScrollView".encodeUtf16()
    let titleW      = "".encodeUtf16()
    let outer = CreateWindowExW(
        0,
        outerClassW, titleW, style,
        x as i32, y as i32, width as i32, height as i32,
        parent, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
    )

    // Inner pane: visible child, initially sized to the viewport.
    // `setScrollViewContentSize` grows it as children land.
    let innerClassW = "IlangGuiScrollViewInner".encodeUtf16()
    let innerStyle: u32 = (WindowStyle.child as u32) | (WindowStyle.visible as u32)
    let inner = CreateWindowExW(
        0,
        innerClassW, titleW, innerStyle,
        0 as i32, 0 as i32, width as i32, height as i32,
        outer, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
    )

    let handle = outer as i64
    let slot = new ScrollViewSlot(outer, inner, width as i32, height as i32)
    SCROLL_VIEWS[handle] = slot
    updateScrollInfo(slot)
    handle
}

pub fn scrollViewDocumentHandle(handle: i64): i64 {
    match SCROLL_VIEWS.get(handle) {
        some(slot) { slot.inner as i64 }
        none       { -1 as i64 }
    }
}

pub fn setScrollViewFrame(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = SCROLL_VIEWS.get(handle) {
        // The WM_SIZE that follows will re-run `applyScroll`.
        MoveWindow(slot.outer, x as i32, y as i32, w as i32, h as i32, 1 as i32)
    }
}

pub fn setScrollViewVisible(handle: i64, visible: bool) {
    if let some(slot) = SCROLL_VIEWS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.outer, cmd)
    }
}

pub fn setScrollViewEnabled(handle: i64, enabled: bool) {
    if let some(slot) = SCROLL_VIEWS.get(handle) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.outer, flag)
    }
}

/// `ShowScrollBar` flips the bar's visibility post-creation.
/// We don't strip / re-add the WS_HSCROLL / WS_VSCROLL window
/// styles because Win32 keeps reserving the gutter space for a
/// hidden bar — that's actually the behaviour we want (toggling
/// the bar back on shouldn't reshuffle the inner pane's layout).
pub fn setScrollViewShowHScroll(handle: i64, show: bool) {
    if let some(slot) = SCROLL_VIEWS.get(handle) {
        let flag: i32 = if show { 1 as i32 } else { 0 as i32 }
        ShowScrollBar(slot.outer, SB_HORZ, flag)
    }
}
pub fn setScrollViewShowVScroll(handle: i64, show: bool) {
    if let some(slot) = SCROLL_VIEWS.get(handle) {
        let flag: i32 = if show { 1 as i32 } else { 0 as i32 }
        ShowScrollBar(slot.outer, SB_VERT, flag)
    }
}

pub fn setScrollViewContentSize(handle: i64, w: f64, h: f64) {
    if let some(slot) = SCROLL_VIEWS.get(handle) {
        slot.contentW = w as i32
        slot.contentH = h as i32
        applyScroll(slot)
    }
}

} // @extern(C)
