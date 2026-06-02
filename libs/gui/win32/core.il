/// Shared per-window state plus the dispatch entry points the
/// wnd-proc calls.
///
/// This module sits at the bottom of the win32 dependency graph
/// — nothing in it imports any other gui_impl file, so widget
/// modules and `wnd_proc.il` can both depend on it without
/// creating a cycle with `window.il` (which owns
/// `createNativeWindow` and depends on the wnd-proc + every
/// widget's `tryXxx` helper).

use windows {
    HWND, WINDOWPLACEMENT, POINT, RECT,
    WindowLongIndex, GetWindowLongPtrW, GetMenu, AdjustWindowRectEx,
}

/// `lastSizeWParam` is the wParam value (SIZE_* code) of the most
/// recent `WM_SIZE` we delivered, used by `dispatchSize` to
/// disambiguate `SIZE_RESTORED` events that follow a minimise vs.
/// a maximise. `isFullScreen` + `preFullScreenPlacement` +
/// `preFullScreenStyle` together let `setNativeWindowFullScreen`
/// borrow and later restore the window's style + placement when
/// flipping to / from a borderless monitor-cover (Win32 has no
/// native full-screen mode, so we synthesise it).
const SIZE_RESTORED:  i32 = 0
const SIZE_MINIMIZED: i32 = 1
const SIZE_MAXIMIZED: i32 = 2

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
    pub onMinimize:        fn()
    pub onRestore:         fn()
    pub onMaximize:        fn()
    pub onUnmaximize:      fn()
    pub onEnterFullScreen: fn()
    pub onLeaveFullScreen: fn()
    pub onFocus:           fn()
    pub onBlur:            fn()
    pub onMove:            fn(f64, f64)
    pub lastSizeWParam:    i32
    pub isFullScreen:      bool
    pub isClosable:        bool
    pub isMovable:         bool
    pub isAlwaysOnTop:     bool
    pub opacity:           f64
    /// User-supplied content-size constraints. `minW/H` of `0`
    /// = "no minimum"; `hasMaxSize == false` = "no maximum".
    /// `WM_GETMINMAXINFO` reads these and translates content
    /// size → frame size before writing the MINMAXINFO struct.
    pub minW:          f64
    pub minH:          f64
    pub maxW:          f64
    pub maxH:          f64
    pub hasMaxSize:    bool
    pub preFullScreenStyle:     i64
    pub preFullScreenPlacement: WINDOWPLACEMENT

    pub init(hwnd: HWND) {
        this.hwnd = hwnd
        this.onClose           = noOp1
        this.onResize          = noOp2
        this.onKeyDown         = noOpKey
        this.onKeyUp           = noOpKey
        this.onMouseMove       = noOpMouse
        this.onMouseDown       = noOpMouse
        this.onMouseUp         = noOpMouse
        this.onMinimize        = noOp1
        this.onRestore         = noOp1
        this.onMaximize        = noOp1
        this.onUnmaximize      = noOp1
        this.onEnterFullScreen = noOp1
        this.onLeaveFullScreen = noOp1
        this.onFocus           = noOp1
        this.onBlur            = noOp1
        this.onMove            = noOp2
        this.lastSizeWParam    = SIZE_RESTORED
        this.isFullScreen      = false
        this.isClosable        = true
        this.isMovable         = true
        this.isAlwaysOnTop     = false
        this.opacity           = 1.0
        this.minW              = 0.0
        this.minH              = 0.0
        this.maxW              = 0.0
        this.maxH              = 0.0
        this.hasMaxSize        = false
        this.preFullScreenStyle     = 0 as i64
        this.preFullScreenPlacement = WINDOWPLACEMENT {
            length:           44 as u32,
            flags:            0 as u32,
            showCmd:          0 as u32,
            ptMinPosition:    POINT { x: 0 as i32, y: 0 as i32 },
            ptMaxPosition:    POINT { x: 0 as i32, y: 0 as i32 },
            rcNormalPosition: RECT { left: 0, top: 0, right: 0, bottom: 0 },
        }
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

/// Return type for `contentToFrameSize` — the Win32 paths convert
/// the public content-size pair into the frame-size pair needed
/// for `SetWindowPos` / `MINMAXINFO`.
pub class FrameSize {
    pub fw: f64
    pub fh: f64
    pub init(fw: f64, fh: f64) { this.fw = fw; this.fh = fh }
}

/// Compute the frame size (content + chrome) for a given content
/// size by asking Win32 via `AdjustWindowRectEx`. Reads style /
/// extended style / menu state straight off the HWND so callers
/// never have to mirror them.
pub fn contentToFrameSize(hwnd: HWND, contentW: f64, contentH: f64): FrameSize {
    let rect = RECT { left: 0, top: 0, right: contentW as i32, bottom: contentH as i32 }
    let style   = GetWindowLongPtrW(hwnd, WindowLongIndex.style   as i32) as u32
    let exStyle = GetWindowLongPtrW(hwnd, WindowLongIndex.exstyle as i32) as u32
    let hasMenu: i32 = if (GetMenu(hwnd) as i64) != 0 as i64 { 1 } else { 0 }
    AdjustWindowRectEx(&rect, style, hasMenu, exStyle)
    new FrameSize(
        (rect.right - rect.left) as f64,
        (rect.bottom - rect.top) as f64
    )
}

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
pub fn setOnWindowMove(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = WINDOWS.get(handle) { slot.onMove = cb }
}

// ─── dispatch entry points — called by `wnd_proc.guiWndProc` ─

pub fn dispatchClose(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.onClose()
    }
}
/// Win32 packs the new size-state into `wParam` of every `WM_SIZE`
/// (`SIZE_MAXIMIZED` / `SIZE_MINIMIZED` / `SIZE_RESTORED`). The
/// wnd-proc forwards both: `wParam` (as `state`) and the new
/// dimensions. We always dispatch `onResize`, then synthesise the
/// matching transition event whenever `state` differs from the
/// previous `WM_SIZE`'s wParam. `SIZE_RESTORED` after
/// `SIZE_MINIMIZED` is `onRestore`; after `SIZE_MAXIMIZED` is
/// `onUnmaximize`; user-dragged ordinary resizes (restored →
/// restored) emit no transition event.
pub fn dispatchSize(handle: i64, state: i32, w: f64, h: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.onResize(w, h)
        if state != slot.lastSizeWParam {
            let prev = slot.lastSizeWParam
            slot.lastSizeWParam = state
            if state == SIZE_MINIMIZED {
                slot.onMinimize()
            } elif state == SIZE_MAXIMIZED {
                slot.onMaximize()
            } elif state == SIZE_RESTORED {
                if prev == SIZE_MINIMIZED { slot.onRestore() }
                elif prev == SIZE_MAXIMIZED { slot.onUnmaximize() }
            }
        }
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
pub fn dispatchFocus(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) { slot.onFocus() }
}
pub fn dispatchBlur(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) { slot.onBlur() }
}
pub fn dispatchMove(handle: i64, x: f64, y: f64) {
    if let some(slot) = WINDOWS.get(handle) { slot.onMove(x, y) }
}
