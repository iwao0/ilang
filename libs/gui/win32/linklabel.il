/// LinkLabel — Win32 has no plain-text hyperlink class; `SysLink`
/// works but only treats text inside `<a>` tags as click targets,
/// which doesn't match the "the whole label is clickable" surface
/// the public API promises. This is a custom-drawn `IlangGuiLink`
/// window that paints a blue underlined caption, switches the
/// cursor to the hand pointer on hover, and routes click /
/// Space / Enter into `onClick` + `ShellExecuteW`.

use windows {
    HWND, HDC, HBRUSH, HPEN, HFONT, HGDIOBJ,
    HINSTANCE, HMENU, HICON, HCURSOR,
    RECT, PAINTSTRUCT, WNDCLASSEXA, TOOLINFOW, TRACKMOUSEEVENT,
    ClassStyle, WindowStyle, WindowMessage, ShowCmd, VirtualKey,
    BackgroundMode, TrackMouseFlags,
    TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    GetModuleHandleA,
    RegisterClassExW, CreateWindowExW, DefWindowProcW,
    MoveWindow, ShowWindow, SetFocus, GetFocus, EnableWindow,
    GetClientRect, InvalidateRect,
    SendMessageW, TrackMouseEvent,
    BeginPaint, EndPaint, FillRect,
    GetSysColorBrush, DrawFocusRect,
    LoadCursorW, SetCursor,
    SetTextColor, SetBkMode, DrawTextW,
    CreateFontW, SelectObject, DeleteObject,
    ShellExecuteW,
}
use util {
    vkToString, modCtrl, modShift, modAlt, modMeta,
}
use core { windowHwnd }

// DrawTextW format flags.
const DT_LEFT:         u32 = 0x00000000 as u32
const DT_VCENTER:      u32 = 0x00000004 as u32
const DT_SINGLELINE:   u32 = 0x00000020 as u32
const DT_END_ELLIPSIS: u32 = 0x00008000 as u32

const LINK_COLOR:     u32 = 0x00D47800 as u32   // #0078D4 in BGR
const LINK_DISABLED:  u32 = 0x00808080 as u32

class LinkLabelSlot {
    pub hwnd:        HWND
    pub winHandle:   i64
    pub tooltipHwnd: HWND
    pub mouseTracking: bool
    pub text:        string
    pub url:         string
    pub focused:     bool
    pub enabled:     bool
    pub font:        HFONT
    pub onClick:     fn(f64, f64)
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd        = hwnd
        this.winHandle   = winHandle
        this.tooltipHwnd = 0 as HWND
        this.mouseTracking = false
        this.text        = ""
        this.url         = ""
        this.focused     = false
        this.enabled     = true
        this.font        = 0 as HFONT
        this.onClick     = linkLabelNoOp
    }
}

fn linkLabelNoOp(x: f64, y: f64) {}

let LINKLABELS = new Map<i64, LinkLabelSlot>()

// `[false]` until the `IlangGuiLink` window class is registered.
let LINK_CLASS_INITED: bool[] = [false]

// Cached hand cursor — LoadCursor is cheap but doing it on every
// WM_SETCURSOR is wasteful when the value never changes.
let HAND_CURSOR: HCURSOR[] = [0 as HCURSOR]

fn ensureHandCursor(): HCURSOR {
    if HAND_CURSOR[0] as i64 == 0 as i64 {
        // IDC_HAND = 32649.
        HAND_CURSOR[0] = LoadCursorW(0 as HINSTANCE, 32649 as *const u16)
    }
    HAND_CURSOR[0]
}

fn makeLinkFont(): HFONT {
    let faceW = "Segoe UI".encodeUtf16()
    CreateFontW(
        -12 as i32, 0 as i32,
        0 as i32, 0 as i32,
        400,                   // FW_NORMAL
        0 as u32,              // italic
        1 as u32,              // underline
        0 as u32,              // strikeout
        1 as u32,              // DEFAULT_CHARSET
        0 as u32, 0 as u32, 0 as u32, 0 as u32,
        faceW
    )
}

@extern(C) {

fn paintLink(hwnd: HWND, slot: LinkLabelSlot, hdc: HDC) {
    let rc = RECT { left: 0, top: 0, right: 0, bottom: 0 }
    GetClientRect(hwnd, &rc)
    FillRect(hdc, &rc, GetSysColorBrush(15 as i32))  // COLOR_BTNFACE

    let oldFont = SelectObject(hdc, slot.font as HGDIOBJ)
    SetBkMode(hdc, BackgroundMode.transparent as i32)
    SetTextColor(hdc, if slot.enabled { LINK_COLOR } else { LINK_DISABLED })
    let textW = slot.text.encodeUtf16(false)
    DrawTextW(
        hdc, textW, textW.length as i32, &rc,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS
    )
    SelectObject(hdc, oldFont)

    if slot.focused {
        let fr = RECT {
            left: 1 as i32, top: 1 as i32,
            right: rc.right - 1 as i32, bottom: rc.bottom - 1 as i32,
        }
        DrawFocusRect(hdc, &fr)
    }
}

fn clickAndOpen(hwnd: HWND, slot: LinkLabelSlot, x: f64, y: f64) {
    if !slot.enabled { return }
    let cb = slot.onClick
    cb(x, y)
    if slot.url != "" {
        let opW   = "open".encodeUtf16()
        let fileW = slot.url.encodeUtf16()
        ShellExecuteW(
            0 as HWND, opW, fileW,
            0 as *const u16, 0 as *const u16,
            ShowCmd.show as i32
        )
    }
}

pub fn linkWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    let lo = (lParam & 0xFFFF) as f64
    let hi = ((lParam >> 16) & 0xFFFF) as f64
    match msg as WindowMessage {
        WindowMessage.paint {
            if let some(slot) = LINKLABELS.get(hwnd) {
                let ps = new PAINTSTRUCT()
                let hdc = BeginPaint(hwnd as HWND, &ps)
                paintLink(hwnd as HWND, slot, hdc)
                EndPaint(hwnd as HWND, &ps)
            }
            return 0
        }
        WindowMessage.eraseBkgnd {
            return 1
        }
        WindowMessage.setFocus {
            if let some(slot) = LINKLABELS.get(hwnd) {
                slot.focused = true
                InvalidateRect(hwnd as HWND, 0 as *const RECT, 0)
            }
            return 0
        }
        WindowMessage.killFocus {
            if let some(slot) = LINKLABELS.get(hwnd) {
                slot.focused = false
                InvalidateRect(hwnd as HWND, 0 as *const RECT, 0)
            }
            return 0
        }
        WindowMessage.setCursor {
            if let some(slot) = LINKLABELS.get(hwnd) {
                if slot.enabled {
                    SetCursor(ensureHandCursor())
                    return 1
                }
            }
        }
        WindowMessage.mouseMove {
            if let some(slot) = LINKLABELS.get(hwnd) {
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
            if let some(slot) = LINKLABELS.get(hwnd) {
                slot.mouseTracking = false
            }
            return 0
        }
        WindowMessage.lbuttondown {
            if let some(slot) = LINKLABELS.get(hwnd) {
                SetFocus(slot.hwnd)
            }
            return 0
        }
        WindowMessage.lbuttonup {
            if let some(slot) = LINKLABELS.get(hwnd) {
                clickAndOpen(hwnd as HWND, slot, lo, hi)
            }
            return 0
        }
        WindowMessage.keydown {
            if let some(slot) = LINKLABELS.get(hwnd) {
                let vk = wParam as u32 as VirtualKey
                if vk == VirtualKey.space || vk == VirtualKey.returnKey {
                    clickAndOpen(hwnd as HWND, slot, 0.0, 0.0)
                }
            }
            return 0
        }
        _ {}
    }
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

fn registerLinkClass() {
    if LINK_CLASS_INITED[0] { return }
    LINK_CLASS_INITED[0] = true

    let hInstance = GetModuleHandleA(0 as *const char)
    let classNameW = "IlangGuiLink".encodeUtf16()
    let classNamePtr: *const u16 = classNameW
    let wc = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   linkWndProc,
        cbClsExtra:    0,
        cbWndExtra:    0,
        hInstance:     hInstance as HINSTANCE,
        hIcon:         0 as HICON,
        hCursor:       0 as HCURSOR,  // handled in WM_SETCURSOR
        hbrBackground: 0 as HBRUSH,
        lpszMenuName:  0 as *const char,
        lpszClassName: (classNamePtr as i64) as *const char,
        hIconSm:       0 as HICON,
    }
    RegisterClassExW(&wc)
}

pub fn createNativeLinkLabel(
    windowHandle: i64,
    text: string, url: string,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    registerLinkClass()

    let classW = "IlangGuiLink".encodeUtf16()
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
    let handle = hwnd as i64
    let slot = new LinkLabelSlot(hwnd, windowHandle)
    slot.text    = text
    slot.url     = url
    slot.enabled = enabled
    slot.font    = makeLinkFont()
    LINKLABELS[handle] = slot
    handle
}

pub fn setLinkLabelText(handle: i64, text: string) {
    if let some(slot) = LINKLABELS.get(handle) {
        slot.text = text
        InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
    }
}
pub fn setLinkLabelUrl(handle: i64, url: string) {
    if let some(slot) = LINKLABELS.get(handle) { slot.url = url }
}
pub fn setLinkLabelFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = LINKLABELS.get(handle) {
        MoveWindow(slot.hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setLinkLabelEnabled(handle: i64, enabled: bool) {
    if let some(slot) = LINKLABELS.get(handle) {
        slot.enabled = enabled
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
        InvalidateRect(slot.hwnd, 0 as *const RECT, 0)
    }
}
pub fn setLinkLabelVisible(handle: i64, visible: bool) {
    if let some(slot) = LINKLABELS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}
pub fn setLinkLabelTooltip(handle: i64, text: string) {
    if let some(slot) = LINKLABELS.get(handle) {
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
pub fn setOnLinkLabelClick(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = LINKLABELS.get(handle) { slot.onClick = cb }
}

} // @extern(C)
