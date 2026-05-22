/// Shared per-window state plus the dispatch entry points the
/// wnd-proc calls.
///
/// This module sits at the bottom of the win32 dependency graph
/// — nothing in it imports any other gui_impl file, so widget
/// modules and `wnd_proc.il` can both depend on it without
/// creating a cycle with `window.il` (which owns
/// `createNativeWindow` and depends on the wnd-proc + every
/// widget's `tryXxx` helper).

use windows { HWND }

/// One callback bundle per top-level window. Stored in `WINDOWS`
/// keyed by the window's HWND (`hwnd as i64`) — same key gui-core
/// sees as the public `handle: i64`, so both the public setters
/// and the wnd_proc-side dispatch are O(1) Map gets.
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

pub let WINDOWS = new Map<i64, WindowSlot>()

fn noOp1() {}
fn noOp2(w: f64, h: f64) {}
fn noOpKey(c: string, k1: bool, k2: bool, k3: bool, k4: bool) {}
fn noOpMouse(x: f64, y: f64, b: i32, k1: bool, k2: bool, k3: bool, k4: bool) {}

/// Lookup the raw HWND for window `handle`. Used by widget
/// modules' `createNative*` to find the parent HWND for
/// `CreateWindowExW`. Returns `0 as HWND` when `handle` is not a
/// registered window.
pub fn windowHwnd(handle: i64): HWND {
    if let some(slot) = WINDOWS.get(handle) {
        slot.hwnd
    } else {
        0 as HWND
    }
}

/// Number of currently-registered top-level windows.
pub fn windowCount(): i64 { WINDOWS.size() }

// ─── setters (`gui-core` calls these via gui_impl re-exports) ─

pub fn setOnClose(handle: i64, cb: fn()) {
    if let some(slot) = WINDOWS.get(handle) { slot.onClose = cb }
}
pub fn setOnResize(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = WINDOWS.get(handle) { slot.onResize = cb }
}
pub fn setOnKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = WINDOWS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = WINDOWS.get(handle) { slot.onKeyUp = cb }
}
pub fn setOnMouseMove(handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)) {
    if let some(slot) = WINDOWS.get(handle) { slot.onMouseMove = cb }
}
pub fn setOnMouseDown(handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)) {
    if let some(slot) = WINDOWS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnMouseUp(handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)) {
    if let some(slot) = WINDOWS.get(handle) { slot.onMouseUp = cb }
}

// ─── dispatch entry points — called by `wnd_proc.guiWndProc` ─

pub fn dispatchClose(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.onClose()
    }
}
pub fn dispatchSize(handle: i64, w: f64, h: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.onResize(w, h)
    }
}
pub fn dispatchKeyDown(
    handle: i64, code: string, ctrl: bool, shift: bool, alt: bool, meta: bool
) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.onKeyDown(code, ctrl, shift, alt, meta)
    }
}
pub fn dispatchKeyUp(
    handle: i64, code: string, ctrl: bool, shift: bool, alt: bool, meta: bool
) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.onKeyUp(code, ctrl, shift, alt, meta)
    }
}
pub fn dispatchMouseMove(
    handle: i64, x: f64, y: f64, b: i32,
    ctrl: bool, shift: bool, alt: bool, meta: bool
) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.onMouseMove(x, y, b, ctrl, shift, alt, meta)
    }
}
pub fn dispatchMouseDown(
    handle: i64, x: f64, y: f64, b: i32,
    ctrl: bool, shift: bool, alt: bool, meta: bool
) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.onMouseDown(x, y, b, ctrl, shift, alt, meta)
    }
}
pub fn dispatchMouseUp(
    handle: i64, x: f64, y: f64, b: i32,
    ctrl: bool, shift: bool, alt: bool, meta: bool
) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.onMouseUp(x, y, b, ctrl, shift, alt, meta)
    }
}
