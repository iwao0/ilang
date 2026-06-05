/// Panel — bare transparent container HWND. A custom child window
/// class so we can install a wnd-proc that forwards `WM_COMMAND`
/// and `WM_NOTIFY` up to the top-level Window. Without that
/// forwarding, button clicks (which the Win32 BUTTON class delivers
/// as a `WM_COMMAND` to its immediate parent) never reach
/// gui-core's dispatch — buttons hosted inside a Panel look fine
/// but their `onClick` never fires.

use windows {
    HWND, HINSTANCE, HMENU, HICON, HBRUSH, RECT,
    WNDCLASSEXA,
    ClassStyle, WindowStyle, WindowMessage, ShowCmd,
    GetModuleHandleA, GetParent,
    RegisterClassExW, CreateWindowExW, DefWindowProcW,
    SendMessageW, MoveWindow, ShowWindow,
    LoadCursorW,
}
use core { windowHwnd }

let PANEL_CLASS_INITED: bool = false

@extern(C) {

/// Wnd-proc for the custom panel class. The interesting cases:
///   WM_COMMAND / WM_NOTIFY — bubble up to the parent so the
///     top-level Window's wnd-proc can dispatch button / edit /
///     combo / list / tree notifications to the gui-core widgets.
///   WM_ERASEBKGND — return 1 so the parent's already-painted
///     background shows through (no flash on resize).
pub fn panelWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    match msg as WindowMessage {
        WindowMessage.eraseBkgnd { return 1 }
        WindowMessage.command {
            let parent = GetParent(hwnd as HWND)
            if (parent as i64) != 0 as i64 {
                return SendMessageW(parent, msg, wParam, lParam) as i64
            }
        }
        WindowMessage.notify {
            let parent = GetParent(hwnd as HWND)
            if (parent as i64) != 0 as i64 {
                return SendMessageW(parent, msg, wParam, lParam) as i64
            }
        }
        _ {}
    }
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

pub fn registerPanelClass() {
    if PANEL_CLASS_INITED { return }
    PANEL_CLASS_INITED = true
    let hInstance = GetModuleHandleA(0 as *const char)
    let classNameW = "IlangGuiPanel".encodeUtf16()
    let classNamePtr: *const u16 = classNameW
    // IDC_ARROW = 32512.
    let wc = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   panelWndProc,
        cbClsExtra:    0,
        cbWndExtra:    0,
        hInstance:     hInstance as HINSTANCE,
        hIcon:         0 as HICON,
        hCursor:       LoadCursorW(0 as HINSTANCE, 32512 as *const u16),
        hbrBackground: 0 as HBRUSH,
        lpszMenuName:  0 as *const char,
        lpszClassName: (classNamePtr as i64) as *const char,
        hIconSm:       0 as HICON,
    }
    RegisterClassExW(&wc)
}

pub fn createNativePanel(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    registerPanelClass()
    let classW = "IlangGuiPanel".encodeUtf16()
    let titleW = "".encodeUtf16()
    let visibleBits: u32 = if visible {
        WindowStyle.visible as u32
    } else {
        0 as u32
    }
    let style: u32 = (WindowStyle.child as u32) | visibleBits
    let hInstance = GetModuleHandleA(0 as *const char)
    let hwnd = CreateWindowExW(
        0, classW, titleW, style,
        x as i32, y as i32, width as i32, height as i32,
        parent, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
    )
    hwnd as i64
}

pub fn setPanelFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle <= 0 as i64 { return }
    MoveWindow(handle as HWND, x as i32, y as i32, width as i32, height as i32, 1)
}

pub fn setPanelVisible(handle: i64, visible: bool) {
    if handle <= 0 as i64 { return }
    let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
    ShowWindow(handle as HWND, cmd)
}

} // @extern(C)
