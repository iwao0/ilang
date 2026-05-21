/// Windows implementation of the `gui-core` bridge API.
///
/// Mirrors `libs/gui/cocoa/gui_impl.il` — gui-core picks whichever
/// is reachable through `[deps] @target("…")` at build time. The
/// `handle` returned is the parallel-array index, not the raw
/// HWND, so per-window callback lookup goes through the indices
/// stashed in `GWLP_USERDATA`.
///
/// Untested on a real Windows host — the macOS build of the
/// compiler stops at parse/type-check for `@target("windows")`
/// items, so anything here that compiles cleanly on macOS may
/// still misbehave at runtime. Treat the dispatch table /
/// keyCode mapping as a starting point.

use windows {
    GetModuleHandleA,
    HWND, HINSTANCE, HICON, HCURSOR, HBRUSH, HMENU,
    WNDCLASSEXA, MSG, POINT,
    ClassStyle, WindowStyle, ShowCmd, WindowMessage, VirtualKey, WindowLongIndex,
    RegisterClassExA, CreateWindowExA, DefWindowProcA,
    ShowWindow, UpdateWindow, EnableWindow, MoveWindow, SetWindowTextA,
    GetMessageA, TranslateMessage, DispatchMessageA,
    PostQuitMessage,
    GetWindowLongPtrA, SetWindowLongPtrA, GetKeyState,
    LoadCursorA, SystemCursor,
}

// ─── per-window callback table ────────────────────────────────

let HWNDS:          HWND[]                                                  = []
let ON_CLOSE:       fn()[]                                                  = []
let ON_RESIZE:      fn(f64, f64)[]                                          = []
let ON_KEY_DOWN:    fn(string, bool, bool, bool, bool)[]                    = []
let ON_KEY_UP:      fn(string, bool, bool, bool, bool)[]                    = []
let ON_MOUSE_MOVE:  fn(f64, f64, i32, bool, bool, bool, bool)[]             = []
let ON_MOUSE_DOWN:  fn(f64, f64, i32, bool, bool, bool, bool)[]             = []
let ON_MOUSE_UP:    fn(f64, f64, i32, bool, bool, bool, bool)[]             = []

let BUTTON_HWNDS:   HWND[]                                                  = []
let ON_CLICK:       fn(f64, f64, i32)[]                                     = []
fn noOpClick(x: f64, y: f64, b: i32) {
    let _ = x
    let _ = y
    let _ = b
}

fn noOp1() {}
fn noOp2(w: f64, h: f64) {
    let _ = w
    let _ = h
}
fn noOpKey(c: string, k1: bool, k2: bool, k3: bool, k4: bool) {
    let _ = c
    let _ = k1
    let _ = k2
    let _ = k3
    let _ = k4
}
fn noOpMouse(x: f64, y: f64, b: i32, k1: bool, k2: bool, k3: bool, k4: bool) {
    let _ = x
    let _ = y
    let _ = b
    let _ = k1
    let _ = k2
    let _ = k3
    let _ = k4
}

// ─── modifier helpers (GetKeyState high-bit set ⇒ pressed) ─────

fn keyDown(vk: i32): bool {
    (GetKeyState(vk) as i32 & 0x8000) != 0
}
fn modShift(): bool { keyDown(VirtualKey.shift as i32) }
fn modCtrl():  bool { keyDown(VirtualKey.control as i32) }
fn modAlt():   bool { keyDown(VirtualKey.alt as i32) }
fn modMeta():  bool { keyDown(VirtualKey.lWin as i32) || keyDown(VirtualKey.rWin as i32) }

// ─── VirtualKey → W3C KeyboardEvent.code ──────────────────────

fn vkToString(vk: u32): string {
    match vk {
        0x41 { "KeyA" }
        0x42 { "KeyB" }
        0x43 { "KeyC" }
        0x44 { "KeyD" }
        0x45 { "KeyE" }
        0x46 { "KeyF" }
        0x47 { "KeyG" }
        0x48 { "KeyH" }
        0x49 { "KeyI" }
        0x4A { "KeyJ" }
        0x4B { "KeyK" }
        0x4C { "KeyL" }
        0x4D { "KeyM" }
        0x4E { "KeyN" }
        0x4F { "KeyO" }
        0x50 { "KeyP" }
        0x51 { "KeyQ" }
        0x52 { "KeyR" }
        0x53 { "KeyS" }
        0x54 { "KeyT" }
        0x55 { "KeyU" }
        0x56 { "KeyV" }
        0x57 { "KeyW" }
        0x58 { "KeyX" }
        0x59 { "KeyY" }
        0x5A { "KeyZ" }
        0x30 { "Digit0" }
        0x31 { "Digit1" }
        0x32 { "Digit2" }
        0x33 { "Digit3" }
        0x34 { "Digit4" }
        0x35 { "Digit5" }
        0x36 { "Digit6" }
        0x37 { "Digit7" }
        0x38 { "Digit8" }
        0x39 { "Digit9" }
        0x0D { "Enter" }
        0x1B { "Escape" }
        0x08 { "Backspace" }
        0x09 { "Tab" }
        0x20 { "Space" }
        0x25 { "ArrowLeft" }
        0x26 { "ArrowUp" }
        0x27 { "ArrowRight" }
        0x28 { "ArrowDown" }
        _    { "" }
    }
}

@extern(C) {
    /// Window procedure. Every window registered through
    /// `createNativeWindow` shares this class proc; per-instance
    /// callbacks are looked up by the index stored in
    /// `GWLP_USERDATA` (= `WindowLongIndex.userdata`) at create
    /// time.
    pub fn guiWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
        let ix = GetWindowLongPtrA(hwnd as HWND, WindowLongIndex.userdata as i32)
        if ix >= 0 && ix < (ON_CLOSE.length as i64) {
            // lParam packs (x, y) for mouse messages and (width,
            // height) for WM_SIZE — same low/high u16 split, pulled
            // here once so the per-arm code only emits the cb call.
            let lo = (lParam & 0xFFFF) as f64
            let hi = ((lParam >> 16) & 0xFFFF) as f64
            match msg as WindowMessage {
                WindowMessage.close {
                    let cb = ON_CLOSE[ix]
                    cb()
                    PostQuitMessage(0)
                    return 0
                }
                WindowMessage.size {
                    let cb = ON_RESIZE[ix]
                    cb(lo, hi)
                    return 0
                }
                WindowMessage.keydown {
                    let cb = ON_KEY_DOWN[ix]
                    cb(vkToString(wParam as u32),
                       modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.keyup {
                    let cb = ON_KEY_UP[ix]
                    cb(vkToString(wParam as u32),
                       modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.mouseMove {
                    let cb = ON_MOUSE_MOVE[ix]
                    cb(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.lbuttondown {
                    let cb = ON_MOUSE_DOWN[ix]
                    cb(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.lbuttonup {
                    let cb = ON_MOUSE_UP[ix]
                    cb(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.mButtonDown {
                    let cb = ON_MOUSE_DOWN[ix]
                    cb(lo, hi, 1, modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.mButtonUp {
                    let cb = ON_MOUSE_UP[ix]
                    cb(lo, hi, 1, modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.rButtonDown {
                    let cb = ON_MOUSE_DOWN[ix]
                    cb(lo, hi, 2, modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.rButtonUp {
                    let cb = ON_MOUSE_UP[ix]
                    cb(lo, hi, 2, modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.command {
                    // LOWORD(wParam) is the control identifier we
                    // baked in as the button's HMENU value
                    // (= the BUTTON_HWNDS index). BN_CLICKED carries
                    // no cursor; pass (0, 0, 0) and let gui-core
                    // fill in via the cached button position.
                    let id = (wParam & 0xFFFF) as i64
                    if id >= 0 && id < (ON_CLICK.length as i64) {
                        let cb = ON_CLICK[id]
                        cb(0.0, 0.0, 0)
                    }
                    return 0
                }
                _ {}
            }
        }
        DefWindowProcA(hwnd as HWND, msg, wParam, lParam)
    }
}

// ─── public API ────────────────────────────────────────────────
//
// Wrapped in `@extern(C) { ... }` so the raw-pointer literals
// (`0 as *const char` for NULL, `0 as *void`, `&wc` / `&msg`)
// and `cstrFromString` are nameable. ilang only allows raw C
// pointer types to appear inside an `@extern(C)` block.

@extern(C) {

pub fn createNativeWindow(title: string, width: f64, height: f64): i64 {
    let hInstance = GetModuleHandleA(0 as *const char)
    let className = cstrFromString("IlangGuiWindow")
    let titleC = cstrFromString(title)

    // Load IDC_ARROW so Windows restores the standard pointer
    // when the cursor re-enters the client area after the resize
    // border swapped it to a size cursor. Without `hCursor` set,
    // Win32 leaves whatever cursor the previous area was showing.
    let arrowCursor = LoadCursorA(0 as HINSTANCE,
                                  SystemCursor.arrow as i64 as *const char)

    let wc = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   guiWndProc,
        cbClsExtra:    0,
        cbWndExtra:    0,
        hInstance:     hInstance as HINSTANCE,
        hIcon:         0 as HICON,
        hCursor:       arrowCursor,
        hbrBackground: 6 as HBRUSH,         // COLOR_WINDOW (5) + 1
        lpszMenuName:  0 as *const char,
        lpszClassName: className,
        hIconSm:       0 as HICON,
    }
    // RegisterClassExA returns 0 only when the class isn't yet
    // registered; calling twice silently no-ops, which is what we
    // want when `createNativeWindow` is called more than once.
    let _ = RegisterClassExA(&wc)

    let hwnd = CreateWindowExA(
        0,
        className,
        titleC,
        WindowStyle.overlappedWindow as u32,
        100, 100,
        width as i32, height as i32,
        0 as HWND,
        0 as HMENU,
        hInstance as HINSTANCE,
        0 as *void
    )
    let handle: i64 = HWNDS.length as i64
    HWNDS.push(hwnd)
    ON_CLOSE.push(noOp1)
    ON_RESIZE.push(noOp2)
    ON_KEY_DOWN.push(noOpKey)
    ON_KEY_UP.push(noOpKey)
    ON_MOUSE_MOVE.push(noOpMouse)
    ON_MOUSE_DOWN.push(noOpMouse)
    ON_MOUSE_UP.push(noOpMouse)
    // Stash our parallel-array index so WndProc can route back.
    let _ = SetWindowLongPtrA(hwnd, WindowLongIndex.userdata as i32, handle)

    ShowWindow(hwnd, ShowCmd.show as i32)
    UpdateWindow(hwnd)
    handle
}

pub fn setOnClose(handle: i64, cb: fn()) {
    if handle >= 0 && handle < (ON_CLOSE.length as i64) { ON_CLOSE[handle] = cb }
}
pub fn setOnResize(handle: i64, cb: fn(f64, f64)) {
    if handle >= 0 && handle < (ON_RESIZE.length as i64) { ON_RESIZE[handle] = cb }
}
pub fn setOnKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if handle >= 0 && handle < (ON_KEY_DOWN.length as i64) { ON_KEY_DOWN[handle] = cb }
}
pub fn setOnKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if handle >= 0 && handle < (ON_KEY_UP.length as i64) { ON_KEY_UP[handle] = cb }
}
pub fn setOnMouseMove(handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)) {
    if handle >= 0 && handle < (ON_MOUSE_MOVE.length as i64) { ON_MOUSE_MOVE[handle] = cb }
}
pub fn setOnMouseDown(handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)) {
    if handle >= 0 && handle < (ON_MOUSE_DOWN.length as i64) { ON_MOUSE_DOWN[handle] = cb }
}
pub fn setOnMouseUp(handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)) {
    if handle >= 0 && handle < (ON_MOUSE_UP.length as i64) { ON_MOUSE_UP[handle] = cb }
}

// ─── buttons ──────────────────────────────────────────────────

pub fn createNativeButton(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (HWNDS.length as i64) {
        return -1
    }
    let parent = HWNDS[windowHandle]
    let id: i64 = BUTTON_HWNDS.length as i64
    let classN = cstrFromString("BUTTON")
    let textC  = cstrFromString(text)
    let visibleBits: u32 = if visible {
        WindowStyle.visible as u32
    } else {
        0 as u32
    }
    let style: u32 = (WindowStyle.child as u32) | visibleBits
    let hInstance = GetModuleHandleA(0 as *const char)
    let hwnd = CreateWindowExA(
        0,
        classN,
        textC,
        style,
        x as i32, y as i32, width as i32, height as i32,
        parent,
        id as HMENU,           // HMENU slot doubles as the control ID
        hInstance as HINSTANCE,
        0 as *void
    )
    let enabledFlag: i32 = if enabled { 1 } else { 0 }
    EnableWindow(hwnd, enabledFlag)
    BUTTON_HWNDS.push(hwnd)
    ON_CLICK.push(noOpClick)
    id
}

pub fn setOnButtonClick(handle: i64, cb: fn(f64, f64, i32)) {
    if handle >= 0 && handle < (ON_CLICK.length as i64) { ON_CLICK[handle] = cb }
}
pub fn setButtonText(handle: i64, text: string) {
    if handle >= 0 && handle < (BUTTON_HWNDS.length as i64) {
        SetWindowTextA(BUTTON_HWNDS[handle], cstrFromString(text))
    }
}
pub fn setButtonFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (BUTTON_HWNDS.length as i64) {
        MoveWindow(BUTTON_HWNDS[handle],
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setButtonEnabled(handle: i64, enabled: bool) {
    if handle >= 0 && handle < (BUTTON_HWNDS.length as i64) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(BUTTON_HWNDS[handle], flag)
    }
}
pub fn setButtonVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (BUTTON_HWNDS.length as i64) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(BUTTON_HWNDS[handle], cmd)
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
        let r = GetMessageA(&msg, 0 as HWND, 0, 0)
        if r <= 0 { break }
        TranslateMessage(&msg)
        let _ = DispatchMessageA(&msg)
    }
}

} // @extern(C)
