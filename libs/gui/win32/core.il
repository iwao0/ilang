/// Shared per-window state and the dispatch entry points the
/// wnd-proc calls.
///
/// This module sits at the bottom of the win32 dependency graph
/// — nothing in it imports any other gui_impl file, so widget
/// modules and `wnd_proc.il` can both depend on it without
/// creating a cycle with `window.il` (which owns
/// `createNativeWindow` and depends on the wnd-proc + every
/// widget's `tryXxx` helper).

use windows { HWND }

/// One callback bundle per top-level window. Stored in
/// `WINDOWS`, looked up by the parallel-array index we stash
/// into `GWLP_USERDATA` at create time so `guiWndProc` can
/// recover the per-window state from any HWND it's called
/// against.
pub class WindowSlot {
    pub hwnd: HWND
    pub onClose:      fn()
    pub onResize:     fn(f64, f64)
    pub onKeyDown:    fn(string, bool, bool, bool, bool)
    pub onKeyUp:      fn(string, bool, bool, bool, bool)
    pub onMouseMove:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)

    pub init(hwnd: HWND) {
        this.hwnd = hwnd
        this.onClose     = noOp1
        this.onResize    = noOp2
        this.onKeyDown   = noOpKey
        this.onKeyUp     = noOpKey
        this.onMouseMove = noOpMouse
        this.onMouseDown = noOpMouse
        this.onMouseUp   = noOpMouse
    }
}

pub let WINDOWS: WindowSlot[] = []

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

/// Lookup the raw HWND for window slot `handle`. Used by widget
/// modules' `createNative*` to find the parent HWND for
/// `CreateWindowExW`. Returns `0 as HWND` on out-of-range.
pub fn windowHwnd(handle: i64): HWND {
    if handle < 0 || handle >= (WINDOWS.length as i64) { return 0 as HWND }
    WINDOWS[handle].hwnd
}

/// Number of currently-registered top-level windows.
pub fn windowCount(): i64 { WINDOWS.length }

// ─── setters (`gui-core` calls these via gui_impl re-exports) ─

pub fn setOnClose(handle: i64, cb: fn()) {
    if handle >= 0 && handle < (WINDOWS.length as i64) {
        WINDOWS[handle].onClose = cb
    }
}
pub fn setOnResize(handle: i64, cb: fn(f64, f64)) {
    if handle >= 0 && handle < (WINDOWS.length as i64) {
        WINDOWS[handle].onResize = cb
    }
}
pub fn setOnKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if handle >= 0 && handle < (WINDOWS.length as i64) {
        WINDOWS[handle].onKeyDown = cb
    }
}
pub fn setOnKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if handle >= 0 && handle < (WINDOWS.length as i64) {
        WINDOWS[handle].onKeyUp = cb
    }
}
pub fn setOnMouseMove(handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)) {
    if handle >= 0 && handle < (WINDOWS.length as i64) {
        WINDOWS[handle].onMouseMove = cb
    }
}
pub fn setOnMouseDown(handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)) {
    if handle >= 0 && handle < (WINDOWS.length as i64) {
        WINDOWS[handle].onMouseDown = cb
    }
}
pub fn setOnMouseUp(handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)) {
    if handle >= 0 && handle < (WINDOWS.length as i64) {
        WINDOWS[handle].onMouseUp = cb
    }
}

// ─── dispatch entry points — called by `wnd_proc.guiWndProc` ─

pub fn dispatchClose(ix: i64) {
    if ix < 0 || ix >= (WINDOWS.length as i64) { return }
    let cb = WINDOWS[ix].onClose
    cb()
}
pub fn dispatchSize(ix: i64, w: f64, h: f64) {
    if ix < 0 || ix >= (WINDOWS.length as i64) { return }
    let cb = WINDOWS[ix].onResize
    cb(w, h)
}
pub fn dispatchKeyDown(
    ix: i64, code: string, ctrl: bool, shift: bool, alt: bool, meta: bool
) {
    if ix < 0 || ix >= (WINDOWS.length as i64) { return }
    let cb = WINDOWS[ix].onKeyDown
    cb(code, ctrl, shift, alt, meta)
}
pub fn dispatchKeyUp(
    ix: i64, code: string, ctrl: bool, shift: bool, alt: bool, meta: bool
) {
    if ix < 0 || ix >= (WINDOWS.length as i64) { return }
    let cb = WINDOWS[ix].onKeyUp
    cb(code, ctrl, shift, alt, meta)
}
pub fn dispatchMouseMove(
    ix: i64, x: f64, y: f64, b: i32,
    ctrl: bool, shift: bool, alt: bool, meta: bool
) {
    if ix < 0 || ix >= (WINDOWS.length as i64) { return }
    let cb = WINDOWS[ix].onMouseMove
    cb(x, y, b, ctrl, shift, alt, meta)
}
pub fn dispatchMouseDown(
    ix: i64, x: f64, y: f64, b: i32,
    ctrl: bool, shift: bool, alt: bool, meta: bool
) {
    if ix < 0 || ix >= (WINDOWS.length as i64) { return }
    let cb = WINDOWS[ix].onMouseDown
    cb(x, y, b, ctrl, shift, alt, meta)
}
pub fn dispatchMouseUp(
    ix: i64, x: f64, y: f64, b: i32,
    ctrl: bool, shift: bool, alt: bool, meta: bool
) {
    if ix < 0 || ix >= (WINDOWS.length as i64) { return }
    let cb = WINDOWS[ix].onMouseUp
    cb(x, y, b, ctrl, shift, alt, meta)
}
