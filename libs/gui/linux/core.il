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
    pub onMinimize:        fn()
    pub onRestore:         fn()
    pub onMaximize:        fn()
    pub onUnmaximize:      fn()
    pub onEnterFullScreen: fn()
    pub onLeaveFullScreen: fn()
    pub onFocus:           fn()
    pub onBlur:            fn()
    pub onMove:            fn(f64, f64)
    /// Previous `maximized` / `fullscreened` / `minimized` bits,
    /// used by the `notify::` callbacks in window.il to detect
    /// edge transitions (GTK fires `notify` for both rising and
    /// falling edges).
    pub wasMaximized:  bool
    pub wasFullScreen: bool
    pub wasMinimized:  bool
    pub wasActive:     bool
    pub isMovable:     bool
    pub isAlwaysOnTop: bool
    /// Geometry / position state. GTK4 dropped the per-window
    /// position APIs (no `gtk_window_move`, no
    /// `gtk_window_get_position`), and has no built-in upper
    /// resize bound, so most of this is informational only —
    /// stored so the `Window` getters have something to return.
    pub posX: f64
    pub posY: f64
    pub minW: f64
    pub minH: f64
    pub maxW: f64
    pub maxH: f64
    pub hasMaxSize: bool

    pub init(win: GtkWindow, fixed: GtkFixed) {
        this.win               = win
        this.fixed             = fixed
        this.onClose           = noOpClose
        this.onResize          = noOpResize
        this.onKeyDown         = noOpKey
        this.onKeyUp           = noOpKey
        this.onMouseMove       = noOpMouse
        this.onMouseDown       = noOpMouse
        this.onMouseUp         = noOpMouse
        this.onMinimize        = noOpClose
        this.onRestore         = noOpClose
        this.onMaximize        = noOpClose
        this.onUnmaximize      = noOpClose
        this.onEnterFullScreen = noOpClose
        this.onLeaveFullScreen = noOpClose
        this.onFocus           = noOpClose
        this.onBlur            = noOpClose
        this.onMove            = noOpResize
        this.wasMaximized      = false
        this.wasFullScreen     = false
        this.wasMinimized      = false
        this.wasActive         = false
        this.isMovable         = true
        this.isAlwaysOnTop     = false
        this.posX              = 0.0
        this.posY              = 0.0
        this.minW              = 0.0
        this.minH              = 0.0
        this.maxW              = 0.0
        this.maxH              = 0.0
        this.hasMaxSize        = false
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
pub fn setOnWindowMinimize(handle: i64, cb: fn()) {
    if let some(slot) = WINDOWS.get(handle) { slot.onMinimize = cb }
}
pub fn setOnWindowRestore(handle: i64, cb: fn()) {
    if let some(slot) = WINDOWS.get(handle) { slot.onRestore = cb }
}
pub fn setOnWindowMaximize(handle: i64, cb: fn()) {
    if let some(slot) = WINDOWS.get(handle) { slot.onMaximize = cb }
}
pub fn setOnWindowUnmaximize(handle: i64, cb: fn()) {
    if let some(slot) = WINDOWS.get(handle) { slot.onUnmaximize = cb }
}
pub fn setOnWindowEnterFullScreen(handle: i64, cb: fn()) {
    if let some(slot) = WINDOWS.get(handle) { slot.onEnterFullScreen = cb }
}
pub fn setOnWindowLeaveFullScreen(handle: i64, cb: fn()) {
    if let some(slot) = WINDOWS.get(handle) { slot.onLeaveFullScreen = cb }
}
pub fn setOnWindowFocus(handle: i64, cb: fn()) {
    if let some(slot) = WINDOWS.get(handle) { slot.onFocus = cb }
}
pub fn setOnWindowBlur(handle: i64, cb: fn()) {
    if let some(slot) = WINDOWS.get(handle) { slot.onBlur = cb }
}
/// GTK4 has no portable per-window position API, so the
/// callback is stored on the slot for API parity but never
/// fires on Linux. The `onMove` Signal on `Window` is silent
/// here by design.
pub fn setOnWindowMove(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = WINDOWS.get(handle) { slot.onMove = cb }
}
