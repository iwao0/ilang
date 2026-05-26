/// Top-level Win32 window: `createNativeWindow` (the public entry
/// point gui-core calls from `Window.init`) and the
/// `runEventLoop` driver.
///
/// Per-window state and the dispatch entry points the wnd-proc
/// calls live in `core.il`; this module just wires `CreateWindowExW`
/// against the shared `guiWndProc` from `wnd_proc.il`.

use windows {
    GetModuleHandleA,
    HWND, HINSTANCE, HICON, HCURSOR, HBRUSH, HMENU,
    WNDCLASSEXA, MSG, POINT,
    ClassStyle, WindowStyle, ShowCmd,
    RegisterClassExW, CreateWindowExW,
    SetWindowTextW, SetWindowPos,
    SetWindowPosFlags,
    GetMessageW, DispatchMessageW, TranslateMessage,
    ShowWindow, UpdateWindow,
}
use util { enableComctl32V6 }
use core { WindowSlot, WINDOWS }
use wnd_proc { guiWndProc }

@extern(C) {

pub fn createNativeWindow(title: string, width: f64, height: f64): i64 {
    enableComctl32V6()

    let hInstance = GetModuleHandleA(0 as *const char)
    // Class name + title go through UTF-16 so non-ASCII chars
    // round-trip intact. ARC keeps both arrays alive across the
    // RegisterClassExW / CreateWindowExW pair; the kernel copies
    // the class name internally, so the buffer can drop after
    // this fn returns.
    let classNameW = "IlangGuiWindow".encodeUtf16()
    let titleW     = title.encodeUtf16()
    // WNDCLASSEXA.lpszClassName is typed `*const char`, but
    // `RegisterClassExW` reinterprets it as `*const u16`. Pun via
    // the typed pointer slot the snprintf example documents.
    let classNamePtr: *const u16 = classNameW

    let wc = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   guiWndProc,
        cbClsExtra:    0,
        cbWndExtra:    0,
        hInstance:     hInstance as HINSTANCE,
        hIcon:         0 as HICON,
        hCursor:       0 as HCURSOR,
        // COLOR_BTNFACE (15) + 1 = 16 — the default dialog grey.
        hbrBackground: 16 as HBRUSH,
        lpszMenuName:  0 as *const char,
        lpszClassName: (classNamePtr as i64) as *const char,
        hIconSm:       0 as HICON,
    }
    RegisterClassExW(&wc)

    let hwnd = CreateWindowExW(
        0,
        classNameW,
        titleW,
        WindowStyle.overlappedWindow as u32,
        100, 100,
        width as i32, height as i32,
        0 as HWND,
        0 as HMENU,
        hInstance as HINSTANCE,
        0 as *void
    )

    let handle = hwnd as i64
    WINDOWS[handle] = new WindowSlot(hwnd)
    // Created HIDDEN. The user controls first-paint via
    // `Window.show()` so they can mutate title / size / add
    // children before the window flashes on screen.
    handle
}

pub fn setWindowTitle(handle: i64, title: string) {
    if let some(slot) = WINDOWS.get(handle) {
        SetWindowTextW(slot.hwnd, title.encodeUtf16())
    }
}

pub fn setWindowSize(handle: i64, width: f64, height: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        // SWP_NOMOVE | SWP_NOZORDER — resize without changing the
        // window's screen position or Z-order siblings.
        let flags = (SetWindowPosFlags.noMove as u32)
                  | (SetWindowPosFlags.noZOrder as u32)
        SetWindowPos(slot.hwnd, 0 as HWND,
                             0, 0, width as i32, height as i32, flags)
    }
}

pub fn showNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        ShowWindow(slot.hwnd, ShowCmd.show as i32)
        UpdateWindow(slot.hwnd)
    }
}

pub fn hideNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        ShowWindow(slot.hwnd, ShowCmd.hide as i32)
    }
}

pub fn runEventLoop() {
    let msg = MSG {
        hwnd:     0 as HWND,
        message:  0,
        wParam:   0,
        lParam:   0,
        time:     0,
        pt:       POINT { x: 0, y: 0 },
        lPrivate: 0,
    }
    loop {
        let r = GetMessageW(&msg, 0 as HWND, 0, 0)
        if r <= 0 { break }
        TranslateMessage(&msg)
        DispatchMessageW(&msg)
    }
}

} // @extern(C)
