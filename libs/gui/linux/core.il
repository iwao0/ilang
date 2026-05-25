/// Shared GTK 4 backend state. Mirrors cocoa/core.il and
/// win32/core.il so the cross-platform `libs/gui` umbrella sees a
/// uniform `gui_impl` surface.
///
/// The handle returned to gui-core for every native widget is its
/// raw `GtkWidget*` cast to `i64` (same convention as Cocoa's
/// `NSObject` pointer / Win32's `HWND`). Per-widget callback
/// tables key off this i64 so signal handlers can resolve back to
/// the user's callback in O(1).

use gtk { GtkWidget, GtkWindow, GtkApplication, GtkFixed, GMainLoop }
use super.event as event
// ─── per-window state ─────────────────────────────────────────

/// One row per top-level window. `WINDOWS` keys off the GtkWindow
/// pointer (cast to i64) — same value gui-core sees as the public
/// `handle: i64`.
pub class WindowSlot {
    pub win:         GtkWindow
    pub fixed:       GtkFixed       // contentView equivalent
    pub onClose:     fn()
    pub onResize:    fn(f64, f64)
    pub onKeyDown:   fn(string, bool, bool, bool, bool)
    pub onKeyUp:     fn(string, bool, bool, bool, bool)
    pub onMouseMove: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseDown: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:   fn(f64, f64, i32, bool, bool, bool, bool)

    pub init(win: GtkWindow, fixed: GtkFixed) {
        this.win         = win
        this.fixed       = fixed
        this.onClose     = noOpClose
        this.onResize    = noOpResize
        this.onKeyDown   = noOpKey
        this.onKeyUp     = noOpKey
        this.onMouseMove = noOpMouse
        this.onMouseDown = noOpMouse
        this.onMouseUp   = noOpMouse
    }
}

pub let WINDOWS = new Map<i64, WindowSlot>()

/// Singleton GtkApplication. Created on first `createNativeWindow`,
/// owned by the process. (Currently unused — see window.il for the
/// manual gtk_init + GMainLoop path.)
pub let APP_HOLDER: GtkApplication[] = []

/// `[true]` once `gtk_init()` has been called. Window creation
/// runs `gtk_init` lazily on the first invocation.
pub let GTK_INITED: bool[] = [false]

/// Singleton GMainLoop. `runEventLoop` builds one on demand and
/// blocks inside `g_main_loop_run` until the window count drops
/// to zero.
pub let LOOP_HOLDER: GMainLoop[] = []

pub fn noOpClose() {}
pub fn noOpResize(w: f64, h: f64) {}
pub fn noOpKey(c: string, k1: bool, k2: bool, k3: bool, k4: bool) {}
pub fn noOpMouse(x: f64, y: f64, b: i32, k1: bool, k2: bool, k3: bool, k4: bool) {}
pub fn noOpClick(x: f64, y: f64, b: i32) {}
pub fn noOpBool(b: bool) {}
pub fn noOpText(s: string) {}
pub fn noOpF64(v: f64) {}
pub fn noOpI32(i: i32) {}

// ─── per-window event setters ─────────────────────────────────

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
