/// Shared GTK 4 backend state. Mirrors cocoa/core.il and
/// win32/core.il so the cross-platform `libs/gui` umbrella sees a
/// uniform `gui_impl` surface.
///
/// The handle returned to gui-core for every native widget is its
/// raw `GtkWidget*` cast to `i64` (same convention as Cocoa's
/// `NSObject` pointer / Win32's `HWND`). Per-widget callback
/// tables key off this i64 so signal handlers can resolve back to
/// the user's callback in O(1).

use gtk { GtkWidget, GtkWindow, GtkApplication, GtkFixed, GtkScrolledWindow, GMainLoop, g_type_from_name }
use std.ffi { cstrFromString }
// ─── per-window state ─────────────────────────────────────────

/// One row per top-level window. `WINDOWS` keys off the GtkWindow
/// pointer (cast to i64) — same value gui-core sees as the public
/// `handle: i64`.
pub class WindowSlot {
    pub win:         GtkWindow
    pub fixed:       GtkFixed       // contentView equivalent
    /// The scrolled window hosting `fixed`. Its allocation is the
    /// window's real content area (below the CSD title bar) and does
    /// not depend on our own child layout, so the resize handler
    /// measures it — not `win` (includes the title bar) and not
    /// `fixed` (sizes to its children). See window.il.
    pub scroll:      GtkScrolledWindow
    pub onClose:     fn()
    pub onResize:    fn(f64, f64)
    pub onKeyDown:   fn(string, bool, bool, bool, bool)
    pub onKeyUp:     fn(string, bool, bool, bool, bool)
    pub onMouseMove: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseDown: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:   fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseEnter:  fn(f64, f64)
    pub onMouseLeave:  fn(f64, f64)
    pub onMouseWheel:  fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
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
    pub parentHandle:  i64
    pub isModal:       bool
    /// Per-window nested GMainLoop for modal sessions. `0` while
    /// the window is non-modal or hasn't been `show()`n
    /// modally; populated by `runNativeWindowModal` and quit
    /// from `linux_window_destroy`.
    pub modalLoopRaw:  i64
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
    /// Wanted client-area size, kept around so the first-map
    /// handler can compare the actual content allocation against
    /// it and grow the window by the missing CSD decoration
    /// height. Cleared (set to 0) once the correction has been
    /// applied.
    pub targetW: i32
    pub targetH: i32
    pub didFitCorrection: bool

    pub init(win: GtkWindow, fixed: GtkFixed, scroll: GtkScrolledWindow) {
        this.win               = win
        this.fixed             = fixed
        this.scroll            = scroll
        this.onClose           = noOpClose
        this.onResize          = noOpResize
        this.onKeyDown         = noOpKey
        this.onKeyUp           = noOpKey
        this.onMouseMove       = noOpMouse
        this.onMouseDown       = noOpMouse
        this.onMouseUp         = noOpMouse
        this.onMouseEnter      = noOpEnterLeave
        this.onMouseLeave      = noOpEnterLeave
        this.onMouseWheel      = noOpWheel
        this.onContextMenu     = noOpMouse
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
        this.parentHandle      = 0 as i64
        this.isModal           = false
        this.modalLoopRaw      = 0 as i64
        this.posX              = 0.0
        this.posY              = 0.0
        this.minW              = 0.0
        this.minH              = 0.0
        this.maxW              = 0.0
        this.maxH              = 0.0
        this.hasMaxSize        = false
        this.targetW           = 0 as i32
        this.targetH           = 0 as i32
        this.didFitCorrection  = false
    }
}

pub let WINDOWS = new Map<i64, WindowSlot>()

/// `GtkParent` — the pair every widget needs from its parent
/// container: a `GtkFixed` to position the widget inside, and the
/// top-level `GtkWindow` for focus / default-widget operations.
/// Both `Window` (direct children land in `WindowSlot.fixed`) and
/// `ScrollView` (children land in the scrolled window's inner
/// fixed) publish themselves through `resolveGtkParent`.
pub class GtkParent {
    pub fixed: GtkFixed
    pub win:   GtkWindow
    pub init(fixed: GtkFixed, win: GtkWindow) {
        this.fixed = fixed
        this.win   = win
    }
}

/// Per-non-window-container `GtkParent` table — `ScrollView`'s
/// inner-fixed handle maps here so widgets parented under it find
/// the right GtkFixed without going through WINDOWS.
pub let GTK_PARENTS = new Map<i64, GtkParent>()

/// Resolve a `parentHandle` (passed to every `createNative*`) to
/// the `GtkParent` that should host the new widget. Returns `none`
/// when the handle isn't a registered Window OR a registered
/// non-window container, so the caller can fail-soft with
/// `return -1`.
pub fn resolveGtkParent(handle: i64): GtkParent? {
    if let some(slot) = WINDOWS.get(handle) {
        return some(new GtkParent(slot.fixed, slot.win))
    }
    if let some(p) = GTK_PARENTS.get(handle) {
        return some(p)
    }
    none
}

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
/// Zero-arg no-op shared with the cocoa / win32 backends under the
/// `noOp1` name; widget slots use it for `onFocus` / `onBlur` defaults.
pub fn noOp1() {}
pub fn noOpResize(w: f64, h: f64) {}
pub fn noOpKey(c: string, k1: bool, k2: bool, k3: bool, k4: bool) {}
pub fn noOpMouse(x: f64, y: f64, b: i32, k1: bool, k2: bool, k3: bool, k4: bool) {}
pub fn noOpEnterLeave(x: f64, y: f64) {}
pub fn noOpWheel(dx: f64, dy: f64, x: f64, y: f64, k1: bool, k2: bool, k3: bool, k4: bool) {}
pub fn noOpClick(x: f64, y: f64, b: i32) {}
pub fn noOpBool(b: bool) {}
pub fn noOpText(s: string) {}
pub fn noOpF64(v: f64) {}
pub fn noOpI32(i: i32) {}
pub fn noOpClickMod(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {}

/// `state & mask != 0`, shared by every widget's GDK key/click
/// handler so they can test `GdkModifierType` bits uniformly.
pub fn hasMod(state: u32, mask: u32): bool { (state & mask) != 0 as u32 }

/// GDK button numbering → W3C `MouseEvent.button`. GDK uses 1
/// for primary, 2 for middle, 3 for secondary; the DOM uses
/// 0 / 1 / 2 respectively. Unknown buttons fall through to 0.
pub fn gdkBtnToW3c(gdk: u32): i32 {
    match gdk {
        1 { 0 as i32 }
        2 { 1 as i32 }
        3 { 2 as i32 }
        _ { 0 as i32 }
    }
}

@extern(C) {
    /// Runtime `GType` for `GtkStringObject`, used to type the
    /// `GListStore`s that back the tree / table row models. Shared
    /// here because both wrappers need the same lookup; `g_type_from_name`
    /// is a `@lib` extern so the call has to live in an `@extern` block.
    pub fn stringObjectType(): u64 {
        g_type_from_name(cstrFromString("GtkStringObject"))
    }
}

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
pub fn setOnWindowMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = WINDOWS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnWindowMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = WINDOWS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnWindowMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = WINDOWS.get(handle) { slot.onMouseWheel = cb }
}
pub fn setOnWindowContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = WINDOWS.get(handle) { slot.onContextMenu = cb }
}
/// Window-level drop is wired on cocoa / win32; linux is
/// deferred to Round 2 (GtkDropTarget + GdkFileList wiring is
/// non-trivial to bind cleanly). Stub keeps the cross-platform
/// symbol surface intact.
pub fn setOnWindowFilesDropped(handle: i64, cb: fn(string[], f64, f64)) {}
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
