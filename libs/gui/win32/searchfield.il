/// SearchField — Win32 has no native search input, so this is a
/// custom-drawn `IlangGuiSearchField` host window that paints a
/// border + a small ✕ cancel button on the right and embeds an
/// `EDIT` child for the actual text entry.
///
/// Wiring:
/// - `EN_CHANGE` from the child EDIT is the parent-side hook for
///   `onChange`.
/// - The EDIT is sub-classed so a `VK_RETURN` press fires
///   `onSearch` instead of being eaten by the dialog default-
///   button machinery.
/// - Clicking inside the ✕ rect blanks the EDIT and fires
///   `onClear` (the resulting `EN_CHANGE` is suppressed so it
///   doesn't double-fire `onChange` with `""`).

use windows {
    HWND, HDC, HBRUSH, HFONT, HGDIOBJ,
    HINSTANCE, HMENU, HICON, HCURSOR,
    POINT, RECT, PAINTSTRUCT, WNDCLASSEXA,
    ClassStyle, WindowStyle, WindowMessage, ShowCmd, VirtualKey,
    PenStyle,
    GetModuleHandleA,
    RegisterClassExW, CreateWindowExW, DefWindowProcW,
    MoveWindow, ShowWindow, SetFocus, GetFocus, EnableWindow,
    GetClientRect, InvalidateRect,
    SendMessageW, SendMessageA, GetWindowTextW, GetWindowTextLengthW,
    SetWindowTextW,
    BeginPaint, EndPaint, FillRect,
    CreateSolidBrush, CreatePen, SelectObject, DeleteObject,
    GetStockObject, GetSysColorBrush,
    SetWindowSubclass, DefSubclassProc,
    LoadCursorW,
    Rectangle, MoveToEx, LineTo,
    SetBkMode,
    BackgroundMode, StockObject,
}
use util {
    applyGuiFont, vkToString, modCtrl, modShift, modAlt, modMeta, noOpText,
    ensureGdiplus,
}
use gdiplus {
    GpGraphics, GpPen,
    SmoothingMode,
    GdipCreateFromHDC, GdipDeleteGraphics, GdipSetSmoothingMode,
    GdipCreatePen1, GdipDeletePen,
    GdipSetPenStartCap, GdipSetPenEndCap,
    GdipDrawEllipse, GdipDrawLine,
}
use core { windowHwnd, noOp1, noOpKey }

class SearchFieldSlot {
    pub host:      HWND
    pub edit:      HWND
    pub winHandle: i64
    pub focused:   bool
    pub enabled:   bool
    /// `true` between "user clicked ✕" and "EDIT raised the resulting
    /// EN_CHANGE", so the change handler can swallow the synthetic
    /// "set to empty" notification and avoid double-firing
    /// `onChange("")` on top of `onClear`.
    pub suppressNextChange: bool
    pub onChange:  fn(string)
    pub onSearch:  fn(string)
    pub onClear:   fn()
    pub onFocus:   fn()
    pub onBlur:    fn()
    pub onKeyDown: fn(string, bool, bool, bool, bool)
    pub onKeyUp:   fn(string, bool, bool, bool, bool)
    pub init(host: HWND, edit: HWND, winHandle: i64) {
        this.host = host
        this.edit = edit
        this.winHandle = winHandle
        this.focused = false
        this.enabled = true
        this.suppressNextChange = false
        this.onChange = noOpText
        this.onSearch = noOpText
        this.onClear  = noOp1
        this.onFocus  = noOp1
        this.onBlur   = noOp1
        this.onKeyDown = noOpKey
        this.onKeyUp   = noOpKey
    }
}

let SEARCHFIELDS = new Map<i64, SearchFieldSlot>()
let EDIT_TO_HOST = new Map<i64, i64>()  // child EDIT hwnd → host hwnd

let SEARCH_CLASS_INITED: bool = false

const SEARCH_ICON_W: i32 = 22 as i32
const CANCEL_BTN_W:  i32 = 20 as i32
const EDIT_PAD_T:    i32 = 5 as i32
const EDIT_PAD_B:    i32 = 1 as i32
const BORDER:        u32 = 0x00B0B0B0 as u32
const CANCEL_FG:     u32 = 0x00606060 as u32
const CANCEL_FG_DIS: u32 = 0x00B0B0B0 as u32
// 0xFF6B6B6B — neutral mid-grey shared with the accordion chevron.
const ICON_ARGB:     u32 = 0xFF6B6B6B as u32
const ICON_ARGB_DIS: u32 = 0xFFB0B0B0 as u32

@extern(C) {

fn editRectFor(slot: SearchFieldSlot, w: i32, h: i32): RECT {
    RECT {
        left:   SEARCH_ICON_W,
        top:    EDIT_PAD_T,
        right:  w - CANCEL_BTN_W,
        bottom: h - EDIT_PAD_B,
    }
}

fn cancelRectFor(w: i32, h: i32): RECT {
    let cy = h / 2 as i32
    let r = 6 as i32
    let cx = w - CANCEL_BTN_W / 2 as i32
    RECT {
        left:   cx - r,
        top:    cy - r,
        right:  cx + r,
        bottom: cy + r,
    }
}

fn paintSearchField(hwnd: HWND, slot: SearchFieldSlot, hdc: HDC) {
    let rc = RECT { left: 0, top: 0, right: 0, bottom: 0 }
    GetClientRect(hwnd, &rc)
    let w = rc.right - rc.left
    let h = rc.bottom - rc.top

    // Background fill so the area around the EDIT is white-ish
    // (matches the EDIT control's own background).
    FillRect(hdc, &rc, GetStockObject(StockObject.whiteBrush as i32) as HBRUSH)

    // 1px outer border.
    let pen = CreatePen(PenStyle.solid as i32, 1, BORDER)
    let oldPen = SelectObject(hdc, pen as HGDIOBJ)
    let oldBrush = SelectObject(hdc, GetStockObject(StockObject.nullBrush as i32))
    Rectangle(hdc, 0 as i32, 0 as i32, w, h)
    SelectObject(hdc, oldPen)
    SelectObject(hdc, oldBrush)
    DeleteObject(pen as HGDIOBJ)

    // Magnifying-glass icon + ✕ via GDI+ so the strokes are
    // anti-aliased and match the chevron rendering style.
    ensureGdiplus()
    let g: *GpGraphics = 0 as *GpGraphics
    GdipCreateFromHDC(hdc, &g)
    if g as i64 != 0 as i64 {
        GdipSetSmoothingMode(g, SmoothingMode.antiAlias as i32)
        let argb: u32 = if slot.enabled { ICON_ARGB } else { ICON_ARGB_DIS }
        let glyphPen: *GpPen = 0 as *GpPen
        GdipCreatePen1(argb, 1.4 as f32, 0 as i32, &glyphPen)
        // LineCap.Round = 2 — keeps the handle stub from looking
        // chipped at small sizes.
        GdipSetPenStartCap(glyphPen, 2 as i32)
        GdipSetPenEndCap(glyphPen, 2 as i32)

        // Magnifier lens — a circle centred slightly left of the
        // icon-area centre with a 4 px radius. Followed by the
        // short diagonal handle off the bottom-right.
        let icx: f64 = 9.0
        // Lens centre lifted ~2 px above the geometric centre so
        // the handle's mass below doesn't drag the visual centre
        // down.
        let icy: f64 = (h as f64) / 2.0 - 2.0
        let r:   f64 = 4.0
        GdipDrawEllipse(g, glyphPen,
            (icx - r) as f32, (icy - r) as f32,
            (2.0 * r) as f32, (2.0 * r) as f32)
        // Handle: from a point on the circle's lower-right
        // tangent down to a stub end ~3 px away.
        let hx1: f64 = icx + r * 0.707
        let hy1: f64 = icy + r * 0.707
        let hx2: f64 = hx1 + 3.0
        let hy2: f64 = hy1 + 3.0
        GdipDrawLine(g, glyphPen,
            hx1 as f32, hy1 as f32, hx2 as f32, hy2 as f32)

        // ✕ on the right when the field has any text.
        let len = GetWindowTextLengthW(slot.edit)
        if len > 0 as i32 {
            let cr = cancelRectFor(w, h)
            // Pull the strokes in 1 px from the hit-rect edges so
            // the visible glyph stays comfortably inside the
            // 12 px box.
            let x1: f64 = (cr.left + 2 as i32) as f64
            let y1: f64 = (cr.top  + 2 as i32) as f64
            let x2: f64 = (cr.right - 2 as i32) as f64
            let y2: f64 = (cr.bottom - 2 as i32) as f64
            GdipDrawLine(g, glyphPen, x1 as f32, y1 as f32, x2 as f32, y2 as f32)
            GdipDrawLine(g, glyphPen, x2 as f32, y1 as f32, x1 as f32, y2 as f32)
        }

        GdipDeletePen(glyphPen)
        GdipDeleteGraphics(g)
    }
}

fn hitCancel(slot: SearchFieldSlot, x: i32, y: i32): bool {
    let rc = RECT { left: 0, top: 0, right: 0, bottom: 0 }
    GetClientRect(slot.host, &rc)
    let w = rc.right - rc.left
    let h = rc.bottom - rc.top
    // Reserve the right CANCEL_BTN_W column as the hit-zone — bigger
    // than the visible ✕ so it stays comfortable to click.
    x >= w - CANCEL_BTN_W && x < w
}

fn readEditText(edit: HWND): string {
    let len = GetWindowTextLengthW(edit)
    if len <= 0 as i32 { return "" }
    // Build a u16 buffer of length+1 and let GetWindowTextW fill it.
    let buf: u16[] = []
    let i = 0 as i32
    while i < len + 1 as i32 {
        buf.push(0 as u16)
        i = i + 1 as i32
    }
    GetWindowTextW(edit, buf, (len + 1 as i32))
    string.fromUtf16(buf.slice(0 as i64, len as i64))
}

/// EDIT subclass — only catches WM_KEYDOWN VK_RETURN so we can
/// fire `onSearch` (the dialog default-button path would
/// otherwise eat it).
pub fn searchFieldEditSubclass(
    hwnd: i64, msg: u32, wParam: u64, lParam: i64,
    idSubclass: u64, refData: u64
): i64 {
    match msg as WindowMessage {
        WindowMessage.keydown {
            let vk = wParam as u32 as VirtualKey
            if vk == VirtualKey.enter {
                if let some(hostI) = EDIT_TO_HOST.get(hwnd) {
                    if let some(slot) = SEARCHFIELDS.get(hostI) {
                        slot.onSearch(readEditText(slot.edit))
                    }
                }
                return 0
            }
            if let some(hostI) = EDIT_TO_HOST.get(hwnd) {
                if let some(slot) = SEARCHFIELDS.get(hostI) {
                    slot.onKeyDown(vkToString(vk), modCtrl(), modShift(), modAlt(), modMeta())
                }
            }
        }
        WindowMessage.keyup {
            if let some(hostI) = EDIT_TO_HOST.get(hwnd) {
                if let some(slot) = SEARCHFIELDS.get(hostI) {
                    let vk = wParam as u32 as VirtualKey
                    slot.onKeyUp(vkToString(vk), modCtrl(), modShift(), modAlt(), modMeta())
                }
            }
        }
        WindowMessage.setFocus {
            if let some(hostI) = EDIT_TO_HOST.get(hwnd) {
                if let some(slot) = SEARCHFIELDS.get(hostI) {
                    slot.focused = true
                    InvalidateRect(slot.host, 0 as *const RECT, 0)
                    slot.onFocus()
                }
            }
        }
        WindowMessage.killFocus {
            if let some(hostI) = EDIT_TO_HOST.get(hwnd) {
                if let some(slot) = SEARCHFIELDS.get(hostI) {
                    slot.focused = false
                    InvalidateRect(slot.host, 0 as *const RECT, 0)
                    slot.onBlur()
                }
            }
        }
        _ {}
    }
    DefSubclassProc(hwnd as HWND, msg, wParam, lParam)
}

pub fn searchFieldWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    let lo = (lParam & 0xFFFF) as i32
    let hi = ((lParam >> 16) & 0xFFFF) as i32
    match msg as WindowMessage {
        WindowMessage.paint {
            if let some(slot) = SEARCHFIELDS.get(hwnd) {
                let ps = new PAINTSTRUCT()
                let hdc = BeginPaint(hwnd as HWND, &ps)
                paintSearchField(hwnd as HWND, slot, hdc)
                EndPaint(hwnd as HWND, &ps)
            }
            return 0
        }
        WindowMessage.eraseBkgnd {
            return 1
        }
        WindowMessage.size {
            if let some(slot) = SEARCHFIELDS.get(hwnd) {
                let w = (lParam & 0xFFFF) as i32
                let h = ((lParam >> 16) & 0xFFFF) as i32
                let er = editRectFor(slot, w, h)
                MoveWindow(slot.edit,
                    er.left, er.top,
                    er.right - er.left, er.bottom - er.top, 1)
            }
            return 0
        }
        WindowMessage.command {
            // EN_CHANGE = 0x0300, lives in HIWORD(wParam).
            let notify = ((wParam >> 16) & 0xFFFF) as i32
            if notify == 0x0300 as i32 {
                if let some(slot) = SEARCHFIELDS.get(hwnd) {
                    if slot.suppressNextChange {
                        slot.suppressNextChange = false
                    } else {
                        slot.onChange(readEditText(slot.edit))
                    }
                    InvalidateRect(slot.host, 0 as *const RECT, 0)
                }
                return 0
            }
        }
        WindowMessage.lbuttondown {
            if let some(slot) = SEARCHFIELDS.get(hwnd) {
                if hitCancel(slot, lo, hi) {
                    if GetWindowTextLengthW(slot.edit) > 0 as i32 {
                        slot.suppressNextChange = true
                        let emptyW = "".encodeUtf16()
                        SetWindowTextW(slot.edit, emptyW)
                        InvalidateRect(slot.host, 0 as *const RECT, 0)
                        slot.onClear()
                    }
                } else {
                    let _ = SetFocus(slot.edit)
                }
            }
            return 0
        }
        _ {}
    }
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

fn registerSearchFieldClass() {
    if SEARCH_CLASS_INITED { return }
    SEARCH_CLASS_INITED = true

    let hInstance = GetModuleHandleA(0 as *const char)
    let classNameW = "IlangGuiSearchField".encodeUtf16()
    let classNamePtr: *const u16 = classNameW
    let wc = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   searchFieldWndProc,
        cbClsExtra:    0,
        cbWndExtra:    0,
        hInstance:     hInstance as HINSTANCE,
        hIcon:         0 as HICON,
        hCursor:       LoadCursorW(0 as HINSTANCE, 32513 as *const u16),  // IDC_IBEAM
        hbrBackground: 0 as HBRUSH,
        lpszMenuName:  0 as *const char,
        lpszClassName: (classNamePtr as i64) as *const char,
        hIconSm:       0 as HICON,
    }
    RegisterClassExW(&wc)
}

pub fn createNativeSearchField(
    windowHandle: i64,
    text: string, placeholder: string,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    registerSearchFieldClass()

    let classW = "IlangGuiSearchField".encodeUtf16()
    let titleW = "".encodeUtf16()
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    let disabledBits: u32 = if !enabled { WindowStyle.disabled as u32 } else { 0 as u32 }
    // WS_TABSTOP for focusability.
    let style: u32 = (WindowStyle.child as u32) | visibleBits | disabledBits
                   | (0x00010000 as u32)
    let hInstance = GetModuleHandleA(0 as *const char)
    let host = CreateWindowExW(
        0, classW, titleW, style,
        x as i32, y as i32, width as i32, height as i32,
        parent, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
    )

    // Child EDIT.
    let editClassW = "EDIT".encodeUtf16()
    let editTextW = text.encodeUtf16()
    let editStyle: u32 = (WindowStyle.child as u32)
                       | (WindowStyle.visible as u32)
                       | (0x0080 as u32)        // ES_AUTOHSCROLL
    let er = editRectFor(new SearchFieldSlot(host, 0 as HWND, windowHandle),
                          width as i32, height as i32)
    let edit = CreateWindowExW(
        0, editClassW, editTextW, editStyle,
        er.left, er.top, er.right - er.left, er.bottom - er.top,
        host, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
    )
    applyGuiFont(edit)

    // Placeholder via EM_SETCUEBANNER = 0x1501.
    if placeholder != "" {
        let phW = placeholder.encodeUtf16()
        let phPtr: *const u16 = phW
        SendMessageW(edit, 0x1501 as u32, 1 as u64, phPtr as i64)
    }

    let handle = host as i64
    let slot = new SearchFieldSlot(host, edit, windowHandle)
    slot.enabled = enabled
    SEARCHFIELDS[handle] = slot
    EDIT_TO_HOST.set(edit as i64, handle)
    SetWindowSubclass(edit, searchFieldEditSubclass, 1 as u64, 0 as u64)

    if !enabled { EnableWindow(edit, 0 as i32) }
    handle
}

pub fn readSearchFieldText(handle: i64): string {
    match SEARCHFIELDS.get(handle) {
        some(slot) { readEditText(slot.edit) }
        none       { "" }
    }
}
pub fn setSearchFieldText(handle: i64, text: string) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        SetWindowTextW(slot.edit, text.encodeUtf16())
    }
}
pub fn setSearchFieldPlaceholder(handle: i64, text: string) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        let phW = if text == "" { "".encodeUtf16() } else { text.encodeUtf16() }
        let phPtr: *const u16 = phW
        SendMessageW(slot.edit, 0x1501 as u32, 1 as u64, phPtr as i64)
    }
}
pub fn setSearchFieldFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        MoveWindow(slot.host, x as i32, y as i32, width as i32, height as i32, 1)
        // WM_SIZE on the host re-positions the EDIT.
    }
}
pub fn setSearchFieldEnabled(handle: i64, enabled: bool) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        slot.enabled = enabled
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.host, flag)
        EnableWindow(slot.edit, flag)
        InvalidateRect(slot.host, 0 as *const RECT, 0)
    }
}
pub fn setSearchFieldVisible(handle: i64, visible: bool) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.host, cmd)
    }
}
pub fn setSearchFieldTooltip(handle: i64, text: string) {
    // Tooltip support deferred — host is a custom class; the
    // tooltip plumbing in other widgets binds against the parent
    // window. Add if/when needed.
}
pub fn setOnSearchFieldChange(handle: i64, cb: fn(string)) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onChange = cb }
}
pub fn setOnSearchFieldSearch(handle: i64, cb: fn(string)) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onSearch = cb }
}
pub fn setOnSearchFieldClear(handle: i64, cb: fn()) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onClear = cb }
}
pub fn setOnSearchFieldFocus(handle: i64, cb: fn()) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onFocus = cb }
}
pub fn setOnSearchFieldBlur(handle: i64, cb: fn()) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onBlur = cb }
}
pub fn setOnSearchFieldKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnSearchFieldKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onKeyUp = cb }
}
pub fn focusSearchField(handle: i64) {
    if let some(slot) = SEARCHFIELDS.get(handle) { SetFocus(slot.edit) }
}
pub fn blurSearchField(handle: i64) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        if (GetFocus() as i64) == (slot.edit as i64) {
            SetFocus(slot.winHandle as HWND)
        }
    }
}

} // @extern(C)
