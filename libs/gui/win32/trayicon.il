/// System-tray icon — Win32 backend. The tray needs a window to
/// route its notification callbacks to, but our app may not
/// have any visible window of its own (the tray icon is often
/// the only UI), so we spin up a hidden `IlangGuiTrayMsgWnd`
/// message window per icon. The tray gets `WM_TRAY_CALLBACK`
/// for every mouse event; we parse the lParam-encoded message
/// id to pick between click and menu pop-up.

use windows {
    HWND, HMENU, HINSTANCE, HBRUSH, HICON, HCURSOR,
    POINT, RECT, WNDCLASSEXA,
    ClassStyle, WindowStyle, WindowMessage,
    GetModuleHandleA, RegisterClassExW, CreateWindowExW,
    DefWindowProcW, DestroyWindow,
    Shell_NotifyIconW, LoadImageW, LoadIconW, SystemIcon,
    SetForegroundWindow, GetCursorPos, TrackPopupMenu, TrackPopupMenuFlags,
}
use core { windowHwnd }
use std.ffi { writeU16 }

// Shell_NotifyIcon `dwMessage` values.
const NIM_ADD:        u32 = 0x00000000 as u32
const NIM_MODIFY:     u32 = 0x00000001 as u32
const NIM_DELETE:     u32 = 0x00000002 as u32
const NIM_SETVERSION: u32 = 0x00000004 as u32

// NOTIFYICONDATAW `uFlags` bits.
const NIF_MESSAGE: u32 = 0x00000001 as u32
const NIF_ICON:    u32 = 0x00000002 as u32
const NIF_TIP:     u32 = 0x00000004 as u32

const WM_TRAY_CALLBACK: u32 = 0x0401 as u32   // WM_USER + 1

const IMAGE_ICON:    u32 = 1 as u32
const LR_LOADFROMFILE: u32 = 0x00000010 as u32
const LR_DEFAULTSIZE:  u32 = 0x00000040 as u32

/// NOTIFYICONDATAW — Vista+ layout (cbSize = 976).
pub struct NOTIFYICONDATAW {
    cbSize:            u32
    _pad0:             u32     // align hWnd to 8
    hWnd:              HWND
    uID:               u32
    uFlags:            u32
    uCallbackMessage:  u32
    _pad1:             u32     // align hIcon to 8
    hIcon:             HICON
    szTip:             u16[128]
    dwState:           u32
    dwStateMask:       u32
    szInfo:            u16[256]
    uTimeoutOrVersion: u32
    szInfoTitle:       u16[64]
    dwInfoFlags:       u32
    guidItem:          u8[16]
    hBalloonIcon:      HICON
}

pub class TrayIconSlot {
    pub msgHwnd:    HWND
    pub uID:        u32
    pub menuHandle: i64
    pub onClick:    fn()
    pub init(msgHwnd: HWND, uID: u32) {
        this.msgHwnd    = msgHwnd
        this.uID        = uID
        this.menuHandle = -1 as i64
        this.onClick    = trayNoOp
    }
}

fn trayNoOp() {}

pub let TRAY_ICONS = new Map<i64, TrayIconSlot>()
/// Reverse: message HWND → tray handle, so the WndProc finds
/// the slot without rescanning.
pub let TRAY_BY_MSG_HWND = new Map<i64, i64>()
let NEXT_TRAY_ID: u32 = 1 as u32
let TRAY_CLASS_INITED: bool = false

@extern(C) {

/// Copy a UTF-8 ilang string into a fixed-size `u16` buffer
/// inside the NOTIFYICONDATAW, NUL-terminating and truncating
/// as needed. Caller passes the field's offset within the
/// struct + the buffer's capacity in u16 units.
fn writeFixedString(nidRaw: i64, off: i64, capacity: i64, text: string) {
    let src = text.encodeUtf16(false)
    let n = src.length
    let max = capacity - 1 as i64
    let limit = if n < max { n } else { max }
    let basePtr = nidRaw as *void
    let i = 0 as i64
    while i < limit {
        writeU16(basePtr, off + i * 2 as i64, src[i])
        i = i + 1
    }
    writeU16(basePtr, off + limit * 2 as i64, 0 as u16)
}

pub fn trayMsgWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    if msg == WM_TRAY_CALLBACK {
        if let some(handle) = TRAY_BY_MSG_HWND.get(hwnd) {
            if let some(slot) = TRAY_ICONS.get(handle) {
                let event = (lParam & 0xFFFF) as u32
                if event == WindowMessage.lbuttonup as u32 {
                    slot.onClick()
                }
                if event == WindowMessage.rButtonUp as u32 && slot.menuHandle >= 0 as i64 {
                    let pt = POINT { x: 0 as i32, y: 0 as i32 }
                    GetCursorPos(&pt)
                    SetForegroundWindow(hwnd as HWND)
                    TrackPopupMenu(
                        slot.menuHandle as HMENU,
                        TrackPopupMenuFlags.rightButton as u32,
                        pt.x, pt.y, 0 as i32, hwnd as HWND, 0 as *const RECT
                    )
                }
            }
        }
        return 0
    }
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

fn registerTrayClass() {
    if TRAY_CLASS_INITED { return }
    TRAY_CLASS_INITED = true
    let hInstance = GetModuleHandleA(0 as *const char)
    let classNameW = "IlangGuiTrayMsgWnd".encodeUtf16()
    let classNamePtr: *const u16 = classNameW
    let wc = WNDCLASSEXA {
        cbSize:        80,
        style:         0 as u32,
        lpfnWndProc:   trayMsgWndProc,
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

fn loadIcon(path: string): HICON {
    // Empty path → fall back to the shared system IDI_APPLICATION
    // icon (32512). Shell_NotifyIconW silently drops the icon
    // when hIcon is NULL and `uFlags` includes NIF_ICON, which
    // would leave the tray entry invisible.
    if path == "" {
        return LoadIconW(0 as HINSTANCE, 32512 as *const u16)
    }
    let pathW = path.encodeUtf16()
    let raw = LoadImageW(
        0 as *void, pathW,
        IMAGE_ICON, 0 as i32, 0 as i32,
        LR_LOADFROMFILE | LR_DEFAULTSIZE
    )
    raw as HICON
}

pub fn createNativeTrayIcon(
    iconPath: string, tooltip: string, menuHandle: i64
): i64 {
    registerTrayClass()

    // Hidden message-only window. HWND_MESSAGE = (HWND)-3 makes
    // the window invisible and outside the z-order; it exists
    // only to receive WM_TRAY_CALLBACK.
    let hInstance = GetModuleHandleA(0 as *const char)
    let classW = "IlangGuiTrayMsgWnd".encodeUtf16()
    let titleW = "".encodeUtf16()
    // HWND_MESSAGE = (HWND)-3 on x64 — write the full 64-bit
    // sign-extended pattern; a 32-bit literal cast to u64 zero-
    // extends instead and produces an invalid HWND, which makes
    // CreateWindowExW fail silently and the subsequent NIM_ADD
    // reject the registration.
    let msgHwnd = CreateWindowExW(
        0,
        classW, titleW, 0 as u32,
        0 as i32, 0 as i32, 0 as i32, 0 as i32,
        0xFFFFFFFFFFFFFFFD as u64 as HWND,   // HWND_MESSAGE
        0 as HMENU, hInstance as HINSTANCE, 0 as *void
    )

    let uID = NEXT_TRAY_ID
    NEXT_TRAY_ID = uID + 1 as u32

    let nid = NOTIFYICONDATAW {
        cbSize:            976 as u32,
        _pad0:             0 as u32,
        hWnd:              msgHwnd,
        uID:               uID,
        uFlags:            NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage:  WM_TRAY_CALLBACK,
        _pad1:             0 as u32,
        hIcon:             loadIcon(iconPath),
        szTip:             [0 as u16; 128],
        dwState:           0 as u32,
        dwStateMask:       0 as u32,
        szInfo:            [0 as u16; 256],
        uTimeoutOrVersion: 0 as u32,
        szInfoTitle:       [0 as u16; 64],
        dwInfoFlags:       0 as u32,
        guidItem:          [0 as u8; 16],
        hBalloonIcon:      0 as HICON,
    }
    // szTip lives at offset 40 in the Vista+ struct, capped at
    // 128 wchars.
    writeFixedString(&nid as i64, 40 as i64, 128 as i64, tooltip)

    Shell_NotifyIconW(NIM_ADD, &nid as *void)

    let handle = msgHwnd as i64
    let slot = new TrayIconSlot(msgHwnd, uID)
    slot.menuHandle = menuHandle
    TRAY_ICONS[handle] = slot
    TRAY_BY_MSG_HWND[handle] = handle
    handle
}

fn modifyTrayIcon(slot: TrayIconSlot, iconPath: string, tooltip: string, flags: u32) {
    let nid = NOTIFYICONDATAW {
        cbSize:            976 as u32,
        _pad0:             0 as u32,
        hWnd:              slot.msgHwnd,
        uID:               slot.uID,
        uFlags:            flags,
        uCallbackMessage:  WM_TRAY_CALLBACK,
        _pad1:             0 as u32,
        hIcon:             if (flags & NIF_ICON) != 0 as u32 {
                               loadIcon(iconPath)
                           } else {
                               0 as HICON
                           },
        szTip:             [0 as u16; 128],
        dwState:           0 as u32,
        dwStateMask:       0 as u32,
        szInfo:            [0 as u16; 256],
        uTimeoutOrVersion: 0 as u32,
        szInfoTitle:       [0 as u16; 64],
        dwInfoFlags:       0 as u32,
        guidItem:          [0 as u8; 16],
        hBalloonIcon:      0 as HICON,
    }
    if (flags & NIF_TIP) != 0 as u32 {
        writeFixedString(&nid as i64, 40 as i64, 128 as i64, tooltip)
    }
    Shell_NotifyIconW(NIM_MODIFY, &nid as *void)
}

pub fn setTrayIconIcon(handle: i64, path: string) {
    if let some(slot) = TRAY_ICONS.get(handle) {
        modifyTrayIcon(slot, path, "", NIF_ICON)
    }
}

pub fn setTrayIconTooltip(handle: i64, text: string) {
    if let some(slot) = TRAY_ICONS.get(handle) {
        modifyTrayIcon(slot, "", text, NIF_TIP)
    }
}

/// Win32 tray icons have no title surface — the shell renders
/// only the registered image. Ignored on this backend.
pub fn setTrayIconTitle(handle: i64, text: string) {}

pub fn setTrayIconMenu(handle: i64, menuHandle: i64) {
    if let some(slot) = TRAY_ICONS.get(handle) {
        slot.menuHandle = menuHandle
    }
}

pub fn showTrayIcon(handle: i64, visible: bool) {
    if let some(slot) = TRAY_ICONS.get(handle) {
        let msg: u32 = if visible { NIM_ADD } else { NIM_DELETE }
        let nid = NOTIFYICONDATAW {
            cbSize:            976 as u32,
            _pad0:             0 as u32,
            hWnd:              slot.msgHwnd,
            uID:               slot.uID,
            uFlags:            NIF_MESSAGE,
            uCallbackMessage:  WM_TRAY_CALLBACK,
            _pad1:             0 as u32,
            hIcon:             0 as HICON,
            szTip:             [0 as u16; 128],
            dwState:           0 as u32,
            dwStateMask:       0 as u32,
            szInfo:            [0 as u16; 256],
            uTimeoutOrVersion: 0 as u32,
            szInfoTitle:       [0 as u16; 64],
            dwInfoFlags:       0 as u32,
            guidItem:          [0 as u8; 16],
            hBalloonIcon:      0 as HICON,
        }
        Shell_NotifyIconW(msg, &nid as *void)
    }
}

pub fn removeNativeTrayIcon(handle: i64) {
    if let some(slot) = TRAY_ICONS.get(handle) {
        let nid = NOTIFYICONDATAW {
            cbSize:            976 as u32,
            _pad0:             0 as u32,
            hWnd:              slot.msgHwnd,
            uID:               slot.uID,
            uFlags:            0 as u32,
            uCallbackMessage:  0 as u32,
            _pad1:             0 as u32,
            hIcon:             0 as HICON,
            szTip:             [0 as u16; 128],
            dwState:           0 as u32,
            dwStateMask:       0 as u32,
            szInfo:            [0 as u16; 256],
            uTimeoutOrVersion: 0 as u32,
            szInfoTitle:       [0 as u16; 64],
            dwInfoFlags:       0 as u32,
            guidItem:          [0 as u8; 16],
            hBalloonIcon:      0 as HICON,
        }
        Shell_NotifyIconW(NIM_DELETE, &nid as *void)
        DestroyWindow(slot.msgHwnd)
        TRAY_ICONS.delete(handle)
        TRAY_BY_MSG_HWND.delete(handle)
    }
}

pub fn setOnTrayIconClick(handle: i64, cb: fn()) {
    if let some(slot) = TRAY_ICONS.get(handle) {
        slot.onClick = cb
    }
}

} // @extern(C)
