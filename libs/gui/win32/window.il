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
    WNDCLASSEXA, MSG, POINT, RECT, WINDOWPLACEMENT,
    ClassStyle, WindowStyle, ShowCmd, SystemMetric, WindowLongIndex, MenuFlags,
    RegisterClassExW, CreateWindowExW,
    SetWindowTextW, SetWindowPos,
    SetWindowPosFlags,
    GetMessageW, DispatchMessageW, TranslateMessage,
    ShowWindow, UpdateWindow,
    IsIconic, IsZoomed,
    GetWindowLongPtrW, SetWindowLongPtrW,
    GetWindowPlacement, SetWindowPlacement,
    GetWindowRect, AdjustWindowRectEx, GetMenu,
    GetSystemMetrics, GetSystemMenu, EnableMenuItem,
    SetForegroundWindow, GetForegroundWindow,
    BringWindowToTop, SetActiveWindow,
    SetLayeredWindowAttributes, WindowStyleEx, LayeredWindowFlags,
}
use util { enableComctl32V6 }
use core { WindowSlot, WINDOWS, contentToFrameSize }
use wnd_proc { guiWndProc }

const WS_OVERLAPPEDWINDOW: u64 = WindowStyle.overlappedWindow as u64
/// `WS_THICKFRAME | WS_MAXIMIZEBOX` — the two style bits that
/// together control "the user can resize / maximise this window".
/// Stripping both gives a fully fixed-size frame.
const RESIZABLE_BITS:      u64 = (WindowStyle.thickframe as u64)
                               | (WindowStyle.maximizeBox as u64)
/// `SC_CLOSE` system-menu identifier. Greyed via `EnableMenuItem`
/// to disable both the menu entry and the title-bar × button.
const SC_CLOSE: u32 = 0xF060 as u32
/// Special HWND sentinels accepted by `SetWindowPos`'s
/// `hwndInsertAfter` slot. `topmost` raises into the always-on-
/// top band; `notTopmost` returns to the normal band.
const HWND_TOPMOST:    i64 = -1 as i64
const HWND_NOTOPMOST:  i64 = -2 as i64

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
        // The public API treats `width` / `height` as the CONTENT
        // size on every backend. Win32 `SetWindowPos` measures the
        // full window (content + chrome), so translate via
        // `AdjustWindowRectEx` using the current style + extended
        // style + menu state.
        let f = contentToFrameSize(slot.hwnd, width, height)
        let flags = (SetWindowPosFlags.noMove as u32)
                  | (SetWindowPosFlags.noZOrder as u32)
        SetWindowPos(slot.hwnd, 0 as HWND, 0, 0, f.fw as i32, f.fh as i32, flags)
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

// ─── window state actions ──────────────────────────────────────

pub fn minimizeNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        ShowWindow(slot.hwnd, ShowCmd.minimize as i32)
    }
}
pub fn restoreNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        ShowWindow(slot.hwnd, ShowCmd.restore as i32)
    }
}
pub fn maximizeNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        if IsZoomed(slot.hwnd) == 0 as i32 {
            ShowWindow(slot.hwnd, ShowCmd.showMaximized as i32)
        }
    }
}
pub fn unmaximizeNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        if IsZoomed(slot.hwnd) != 0 as i32 {
            ShowWindow(slot.hwnd, ShowCmd.restore as i32)
        }
    }
}

/// Win32 has no native full-screen mode — synthesise it by
/// stripping the `WS_OVERLAPPEDWINDOW` decoration bits and
/// covering the primary monitor with `SetWindowPos`. The pre-
/// transition style + placement are stashed in the `WindowSlot`
/// so the leave path can restore both faithfully.
pub fn setNativeWindowFullScreen(handle: i64, on: bool) {
    if let some(slot) = WINDOWS.get(handle) {
        if on == slot.isFullScreen { return }
        if on {
            slot.preFullScreenStyle = GetWindowLongPtrW(
                slot.hwnd, WindowLongIndex.style as i32
            )
            GetWindowPlacement(slot.hwnd, &slot.preFullScreenPlacement)
            let stripped = (slot.preFullScreenStyle as u64) & ~WS_OVERLAPPEDWINDOW
            SetWindowLongPtrW(slot.hwnd, WindowLongIndex.style as i32, stripped as i64)
            let cx = GetSystemMetrics(SystemMetric.cxScreen as i32)
            let cy = GetSystemMetrics(SystemMetric.cyScreen as i32)
            let flags = (SetWindowPosFlags.noZOrder as u32)
                      | (SetWindowPosFlags.frameChanged as u32)
            SetWindowPos(slot.hwnd, 0 as HWND, 0, 0, cx, cy, flags)
            slot.isFullScreen = true
            slot.onEnterFullScreen()
        } else {
            SetWindowLongPtrW(
                slot.hwnd, WindowLongIndex.style as i32, slot.preFullScreenStyle
            )
            SetWindowPlacement(slot.hwnd, &slot.preFullScreenPlacement)
            slot.isFullScreen = false
            slot.onLeaveFullScreen()
        }
    }
}

pub fn isNativeWindowMinimized(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        IsIconic(slot.hwnd) != 0 as i32
    } else {
        false
    }
}
pub fn isNativeWindowMaximized(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        IsZoomed(slot.hwnd) != 0 as i32
    } else {
        false
    }
}
pub fn isNativeWindowFullScreen(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        slot.isFullScreen
    } else {
        false
    }
}

// ─── window-trait flags ────────────────────────────────────────

pub fn setNativeWindowResizable(handle: i64, on: bool) {
    if let some(slot) = WINDOWS.get(handle) {
        let style = GetWindowLongPtrW(slot.hwnd, WindowLongIndex.style as i32) as u64
        let next = if on { style | RESIZABLE_BITS } else { style & ~RESIZABLE_BITS }
        if next != style {
            SetWindowLongPtrW(slot.hwnd, WindowLongIndex.style as i32, next as i64)
            // Force a non-client redraw so the title-bar's resize
            // grip + maximise button reflect the change.
            let flags = (SetWindowPosFlags.noMove   as u32)
                      | (SetWindowPosFlags.noSize   as u32)
                      | (SetWindowPosFlags.noZOrder as u32)
                      | (SetWindowPosFlags.frameChanged as u32)
            SetWindowPos(slot.hwnd, 0 as HWND, 0, 0, 0, 0, flags)
        }
    }
}
pub fn isNativeWindowResizable(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        let style = GetWindowLongPtrW(slot.hwnd, WindowLongIndex.style as i32) as u64
        (style & RESIZABLE_BITS) != 0 as u64
    } else {
        false
    }
}

/// Closable on Win32 = grey out `SC_CLOSE` on the system menu.
/// This disables both the title-bar × button and the Alt+F4 /
/// system-menu close path. `EnableMenuItem` uses `MF_BYCOMMAND`
/// (the default 0 bit) to address by command ID.
pub fn setNativeWindowClosable(handle: i64, on: bool) {
    if let some(slot) = WINDOWS.get(handle) {
        let menu = GetSystemMenu(slot.hwnd, 0 as i32)
        if menu as i64 != 0 as i64 {
            let state = if on {
                MenuFlags.bycommand as u32 | MenuFlags.enabled as u32
            } else {
                MenuFlags.bycommand as u32 | MenuFlags.grayed as u32
            }
            EnableMenuItem(menu, SC_CLOSE, state)
            slot.isClosable = on
        }
    }
}
pub fn isNativeWindowClosable(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        slot.isClosable
    } else {
        true
    }
}

/// Movable on Win32 = a per-slot bool consulted by
/// `wnd_proc.guiWndProc` when `WM_SYSCOMMAND` arrives with
/// `wParam == SC_MOVE`. Win32 has no native fixed-position
/// flag; intercepting the SysCommand blocks both the
/// keyboard-driven move and the title-bar drag path.
pub fn setNativeWindowMovable(handle: i64, on: bool) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.isMovable = on
    }
}
pub fn isNativeWindowMovable(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        slot.isMovable
    } else {
        true
    }
}

// ─── focus / Z-order ───────────────────────────────────────────

pub fn focusNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        SetForegroundWindow(slot.hwnd)
    }
}

/// Win32 has no per-window blur primitive; clearing the active
/// window slot with `SetActiveWindow(NULL)` comes closest.
/// `handle` is accepted for API symmetry — the OS-wide effect
/// doesn't need the per-window slot. Foreground status may still
/// belong to this hwnd until the OS hands it elsewhere.
pub fn blurNativeWindow(handle: i64) {
    SetActiveWindow(0 as HWND)
}

pub fn isNativeWindowFocused(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        (GetForegroundWindow() as i64) == (slot.hwnd as i64)
    } else {
        false
    }
}

pub fn setNativeWindowAlwaysOnTop(handle: i64, on: bool) {
    if let some(slot) = WINDOWS.get(handle) {
        let after = if on { HWND_TOPMOST } else { HWND_NOTOPMOST }
        let flags = (SetWindowPosFlags.noMove as u32)
                  | (SetWindowPosFlags.noSize as u32)
                  | (SetWindowPosFlags.noActivate as u32)
        SetWindowPos(slot.hwnd, after as HWND, 0, 0, 0, 0, flags)
        slot.isAlwaysOnTop = on
    }
}
pub fn isNativeWindowAlwaysOnTop(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        slot.isAlwaysOnTop
    } else {
        false
    }
}

pub fn moveTopNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        BringWindowToTop(slot.hwnd)
    }
}

/// Per-window opacity in [0.0, 1.0]. Win32 needs the
/// `WS_EX_LAYERED` extended-style bit before
/// `SetLayeredWindowAttributes` will apply the alpha; we toggle
/// it in lazily on the first non-trivial opacity call. The
/// current value is cached on the slot so `getOpacity` doesn't
/// have to round-trip the `GetLayeredWindowAttributes` byte.
pub fn setNativeWindowOpacity(handle: i64, v: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        let ex = GetWindowLongPtrW(slot.hwnd, WindowLongIndex.exstyle as i32) as u64
        let layeredBit = WindowStyleEx.layered as u64
        if (ex & layeredBit) == 0 as u64 {
            SetWindowLongPtrW(
                slot.hwnd, WindowLongIndex.exstyle as i32,
                (ex | layeredBit) as i64
            )
        }
        let alpha = (v * 255.0) as u8
        SetLayeredWindowAttributes(
            slot.hwnd, 0 as u32, alpha, LayeredWindowFlags.alpha as u32
        )
        slot.opacity = v
    }
}
pub fn getNativeWindowOpacity(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) {
        slot.opacity
    } else {
        1.0
    }
}

// ─── geometry / position / bounds ──────────────────────────────

pub fn centerNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        let cx = GetSystemMetrics(SystemMetric.cxScreen as i32)
        let cy = GetSystemMetrics(SystemMetric.cyScreen as i32)
        let rect = RECT { left: 0, top: 0, right: 0, bottom: 0 }
        GetWindowRect(slot.hwnd, &rect)
        let w = rect.right - rect.left
        let h = rect.bottom - rect.top
        let x = (cx - w) / 2
        let y = (cy - h) / 2
        let flags = (SetWindowPosFlags.noSize as u32)
                  | (SetWindowPosFlags.noZOrder as u32)
        SetWindowPos(slot.hwnd, 0 as HWND, x, y, 0, 0, flags)
    }
}

pub fn setNativeWindowPosition(handle: i64, x: f64, y: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        let flags = (SetWindowPosFlags.noSize as u32)
                  | (SetWindowPosFlags.noZOrder as u32)
        SetWindowPos(slot.hwnd, 0 as HWND, x as i32, y as i32, 0, 0, flags)
    }
}
pub fn getNativeWindowX(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) {
        let rect = RECT { left: 0, top: 0, right: 0, bottom: 0 }
        GetWindowRect(slot.hwnd, &rect)
        rect.left as f64
    } else {
        0.0
    }
}
pub fn getNativeWindowY(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) {
        let rect = RECT { left: 0, top: 0, right: 0, bottom: 0 }
        GetWindowRect(slot.hwnd, &rect)
        rect.top as f64
    } else {
        0.0
    }
}

/// `(x, y)` is the frame top-left in screen coords; `(w, h)` is
/// the CONTENT size. Translate to frame size and slam the whole
/// thing in one `SetWindowPos` call.
pub fn setNativeWindowBounds(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        let f = contentToFrameSize(slot.hwnd, w, h)
        SetWindowPos(
            slot.hwnd, 0 as HWND,
            x as i32, y as i32,
            f.fw as i32, f.fh as i32,
            (SetWindowPosFlags.noZOrder as u32)
        )
    }
}

pub fn setNativeWindowMinSize(handle: i64, w: f64, h: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.minW = w
        slot.minH = h
    }
}
pub fn getNativeWindowMinWidth(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) { slot.minW } else { 0.0 }
}
pub fn getNativeWindowMinHeight(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) { slot.minH } else { 0.0 }
}
pub fn setNativeWindowMaxSize(handle: i64, w: f64, h: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.maxW = w
        slot.maxH = h
        slot.hasMaxSize = true
    }
}
pub fn getNativeWindowMaxWidth(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) {
        if slot.hasMaxSize { slot.maxW } else { 0.0 }
    } else {
        0.0
    }
}
pub fn getNativeWindowMaxHeight(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) {
        if slot.hasMaxSize { slot.maxH } else { 0.0 }
    } else {
        0.0
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
