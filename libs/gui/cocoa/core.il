/// Shared per-window state plus the AppKit delegate / view
/// subclasses that catch close / resize / key / mouse events.
///
/// Every widget file (`button.il` / `label.il` / …) imports the
/// `WINDOWS` table from this module to find the parent NSWindow
/// the widget was created against.

use cocoa {
    NSObject, NSApplication, NSWindow, NSView, NSScreen,
    NSNotification, NSEvent,
    NSEventModifierFlags, KeyCode, nil,
}
use super.events
// ─── per-window state ─────────────────────────────────────────

/// One row per top-level window. Stored in `WINDOWS` keyed by
/// the NSWindow's raw ObjC pointer (`win.handle`) so both the
/// public-API setters and the delegate / view sender-side
/// lookups are O(1) Map gets.
pub class WindowSlot {
    pub win:         NSWindow
    pub delegate:    NSObject       // GuiDelegate keep-alive
    pub view:        NSObject       // GuiView keep-alive
    pub viewHandle:  i64            // `view.handle` — key for VIEW_TO_WINDOW
    pub onClose:     fn()
    pub onResize:    fn(f64, f64)
    pub onKeyDown:   fn(string, bool, bool, bool, bool)
    pub onKeyUp:     fn(string, bool, bool, bool, bool)
    pub onMouseMove: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseDown: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:   fn(f64, f64, i32, bool, bool, bool, bool)
    /// Window-state event hooks. AppKit fires distinct delegate
    /// callbacks for miniaturize / deminiaturize / enter-full-
    /// screen / exit-full-screen, but NOT for `zoom:` — there's
    /// no `windowDidZoom:`. We track the previous `isZoomed`
    /// state in `wasZoomed` and synthesise `onMaximize` /
    /// `onUnmaximize` whenever a `windowDidResize` notification
    /// flips the bit.
    pub onMinimize:        fn()
    pub onRestore:         fn()
    pub onMaximize:        fn()
    pub onUnmaximize:      fn()
    pub onEnterFullScreen: fn()
    pub onLeaveFullScreen: fn()
    pub onFocus:           fn()
    pub onBlur:            fn()
    pub onMove:            fn(f64, f64)
    pub wasZoomed:         bool

    pub init(win: NSWindow, delegate: NSObject, view: NSObject, viewHandle: i64) {
        this.win               = win
        this.delegate          = delegate
        this.view              = view
        this.viewHandle        = viewHandle
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
        this.wasZoomed         = false
    }
}

/// Public handle = `win.handle` (NSWindow's raw ObjC pointer).
pub let WINDOWS = new Map<i64, WindowSlot>()

/// Reverse index: GuiView's raw ObjC pointer → NSWindow's raw
/// ObjC pointer. The view subclass's key/mouse overrides receive
/// `this` (the GuiView); we look up its window via this table so
/// we don't have to scan every WindowSlot.
let VIEW_TO_WINDOW = new Map<i64, i64>()

pub fn noOp1() {}
pub fn noOp2(w: f64, h: f64) {
}
pub fn noOpKey(c: string, k1: bool, k2: bool, k3: bool, k4: bool) {
}
pub fn noOpMouse(x: f64, y: f64, b: i32, k1: bool, k2: bool, k3: bool, k4: bool) {
}
pub fn noOpClick(x: f64, y: f64, b: i32) {
}
pub fn noOpBool(b: bool) {}
pub fn noOpText(s: string) {}
pub fn noOpF64(v: f64) {}
pub fn noOpI32(i: i32) {}

/// Register the GuiView ↔ NSWindow link so key/mouse dispatch
/// in `GuiView` can resolve `this.handle` to a window handle in
/// constant time.
pub fn registerView(viewHandle: i64, windowHandle: i64) {
    VIEW_TO_WINDOW[viewHandle] = windowHandle
}

// ─── modifier-flag helpers ────────────────────────────────────

pub fn modShift(flags: u64): bool { (flags & (NSEventModifierFlags.shift   as u64)) != 0 }
pub fn modCtrl(flags:  u64): bool { (flags & (NSEventModifierFlags.control as u64)) != 0 }
pub fn modAlt(flags:   u64): bool { (flags & (NSEventModifierFlags.option  as u64)) != 0 }
pub fn modMeta(flags:  u64): bool { (flags & (NSEventModifierFlags.command as u64)) != 0 }

// ─── keyCode → W3C KeyboardEvent.code ─────────────────────────

pub fn keyCodeToString(k: KeyCode): string {
    keyCodeToEnum(k) as string
}

/// Cocoa virtual key codes (`cocoa.KeyCode` from
/// `<HIToolbox/Events.h>`) → the parent package's
/// `events.KeyCode` (W3C `KeyboardEvent.code`). Unmapped keys
/// fall through to `events.KeyCode.unknown`; the caller then
/// serialises that into the discriminant string for the
/// cross-package callback ABI.
pub fn keyCodeToEnum(k: KeyCode): events.KeyCode {
    match k {
        KeyCode.keyA       { events.KeyCode.keyA }
        KeyCode.keyS       { events.KeyCode.keyS }
        KeyCode.keyD       { events.KeyCode.keyD }
        KeyCode.keyF       { events.KeyCode.keyF }
        KeyCode.keyH       { events.KeyCode.keyH }
        KeyCode.keyG       { events.KeyCode.keyG }
        KeyCode.keyZ       { events.KeyCode.keyZ }
        KeyCode.keyX       { events.KeyCode.keyX }
        KeyCode.keyC       { events.KeyCode.keyC }
        KeyCode.keyV       { events.KeyCode.keyV }
        KeyCode.keyB       { events.KeyCode.keyB }
        KeyCode.keyQ       { events.KeyCode.keyQ }
        KeyCode.keyW       { events.KeyCode.keyW }
        KeyCode.keyE       { events.KeyCode.keyE }
        KeyCode.keyR       { events.KeyCode.keyR }
        KeyCode.keyY       { events.KeyCode.keyY }
        KeyCode.keyT       { events.KeyCode.keyT }
        KeyCode.keyO       { events.KeyCode.keyO }
        KeyCode.keyU       { events.KeyCode.keyU }
        KeyCode.keyI       { events.KeyCode.keyI }
        KeyCode.keyP       { events.KeyCode.keyP }
        KeyCode.keyL       { events.KeyCode.keyL }
        KeyCode.keyJ       { events.KeyCode.keyJ }
        KeyCode.keyK       { events.KeyCode.keyK }
        KeyCode.keyN       { events.KeyCode.keyN }
        KeyCode.keyM       { events.KeyCode.keyM }
        KeyCode.key0       { events.KeyCode.digit0 }
        KeyCode.key1       { events.KeyCode.digit1 }
        KeyCode.key2       { events.KeyCode.digit2 }
        KeyCode.key3       { events.KeyCode.digit3 }
        KeyCode.key4       { events.KeyCode.digit4 }
        KeyCode.key5       { events.KeyCode.digit5 }
        KeyCode.key6       { events.KeyCode.digit6 }
        KeyCode.key7       { events.KeyCode.digit7 }
        KeyCode.key8       { events.KeyCode.digit8 }
        KeyCode.key9       { events.KeyCode.digit9 }
        KeyCode.returnKey  { events.KeyCode.enter }
        KeyCode.escape     { events.KeyCode.escape }
        KeyCode.delete     { events.KeyCode.backspace }
        KeyCode.tab        { events.KeyCode.tab }
        KeyCode.space      { events.KeyCode.space }
        KeyCode.leftArrow  { events.KeyCode.arrowLeft }
        KeyCode.rightArrow { events.KeyCode.arrowRight }
        KeyCode.downArrow  { events.KeyCode.arrowDown }
        KeyCode.upArrow    { events.KeyCode.arrowUp }
        _                  { events.KeyCode.unknown }
    }
}

// ─── window delegate (close + resize) ─────────────────────────

pub class GuiDelegate : NSObject {
    pub windowWillClose(n: NSNotification) {
        if let some(slot) = WINDOWS.get(n.notificationObject.handle) {
            let cb = slot.onClose
            cb()
        }
    }

    pub windowDidResize(n: NSNotification) {
        if let some(slot) = WINDOWS.get(n.notificationObject.handle) {
            let frame = slot.win.contentView.frame
            let cb = slot.onResize
            cb(frame.size.width, frame.size.height)
            // AppKit has no `windowDidZoom:` delegate hook — the only
            // signal for a zoom transition is a `windowDidResize`
            // that flips `isZoomed`. Track the previous bit and
            // synthesise the matching callback.
            let nowZoomed = slot.win.isZoomed
            if nowZoomed != slot.wasZoomed {
                slot.wasZoomed = nowZoomed
                if nowZoomed {
                    let mcb = slot.onMaximize
                    mcb()
                } else {
                    let ucb = slot.onUnmaximize
                    ucb()
                }
            }
        }
    }

    pub windowDidMiniaturize(n: NSNotification) {
        if let some(slot) = WINDOWS.get(n.notificationObject.handle) {
            let cb = slot.onMinimize
            cb()
        }
    }

    pub windowDidDeminiaturize(n: NSNotification) {
        if let some(slot) = WINDOWS.get(n.notificationObject.handle) {
            let cb = slot.onRestore
            cb()
        }
    }

    pub windowDidEnterFullScreen(n: NSNotification) {
        if let some(slot) = WINDOWS.get(n.notificationObject.handle) {
            let cb = slot.onEnterFullScreen
            cb()
        }
    }

    pub windowDidExitFullScreen(n: NSNotification) {
        if let some(slot) = WINDOWS.get(n.notificationObject.handle) {
            let cb = slot.onLeaveFullScreen
            cb()
        }
    }

    pub windowDidBecomeKey(n: NSNotification) {
        if let some(slot) = WINDOWS.get(n.notificationObject.handle) {
            let cb = slot.onFocus
            cb()
        }
    }
    pub windowDidResignKey(n: NSNotification) {
        if let some(slot) = WINDOWS.get(n.notificationObject.handle) {
            let cb = slot.onBlur
            cb()
        }
    }

    /// `windowDidMove:` fires after every screen-position change.
    /// AppKit's NSWindow.frame.origin is bottom-left in screen
    /// coordinates, so we invert against the primary screen's
    /// height to hand callers a W3C-style top-left position.
    pub windowDidMove(n: NSNotification) {
        if let some(slot) = WINDOWS.get(n.notificationObject.handle) {
            let wf = slot.win.frame
            let sf = NSScreen.mainScreen().frame
            let topLeftY = sf.size.height - (wf.origin.y + wf.size.height)
            let cb = slot.onMove
            cb(wf.origin.x, topLeftY)
        }
    }
}

/// `applicationShouldTerminateAfterLastWindowClosed:` returns true
/// so NSApp exits its `run` loop as soon as the last visible window
/// closes — no manual `terminate:` call needed from inside
/// `windowWillClose:`. Calling terminate from there used to fire the
/// close notification a second time (terminate iterates
/// `[NSApp windows]` and sends `close:` to each; the closing window
/// is still in that list, so the notification re-fires and
/// `onClose` ran twice).
pub class GuiAppDelegate : NSObject {
    pub applicationShouldTerminateAfterLastWindowClosed(app: NSApplication): bool {
        true
    }
}

// ─── content view (key + mouse) ───────────────────────────────
//
// NSWindow delegates don't see keyDown:/mouseDown: — those go
// through the responder chain to the firstResponder, normally
// the contentView. We replace contentView with a GuiView subclass
// that forwards each event to the per-window callback.

pub class GuiView : NSView {
    pub acceptsFirstResponder(): bool { true }

    /// Flip the contentView's coordinate system so subview positions
    /// (and convertPoint:fromView: results) use a top-left origin —
    /// the Windows / W3C convention the gui-core layer hands us.
    /// Without this, widget y values would land at the bottom of the
    /// window instead of the top.
    pub isFlipped(): bool { true }

    pub keyDown(event: NSEvent) {
        if let some(slot) = this.windowSlot() {
            let cb = slot.onKeyDown
            let flags = event.modifierFlags as u64
            cb(keyCodeToString(event.keyCode), modCtrl(flags), modShift(flags),
               modAlt(flags), modMeta(flags))
        }
    }

    pub keyUp(event: NSEvent) {
        if let some(slot) = this.windowSlot() {
            let cb = slot.onKeyUp
            let flags = event.modifierFlags as u64
            cb(keyCodeToString(event.keyCode), modCtrl(flags), modShift(flags),
               modAlt(flags), modMeta(flags))
        }
    }

    pub mouseMoved(event: NSEvent)     { this.dispatchMouse(event, 0, 0) }
    pub mouseDragged(event: NSEvent)   { this.dispatchMouse(event, 0, 0) }
    pub mouseDown(event: NSEvent)      { this.dispatchMouse(event, 1, 0) }
    pub mouseUp(event: NSEvent)        { this.dispatchMouse(event, 2, 0) }
    pub rightMouseDown(event: NSEvent) { this.dispatchMouse(event, 1, 2) }
    pub rightMouseUp(event: NSEvent)   { this.dispatchMouse(event, 2, 2) }
    pub otherMouseDown(event: NSEvent) { this.dispatchMouse(event, 1, 1) }
    pub otherMouseUp(event: NSEvent)   { this.dispatchMouse(event, 2, 1) }

    /// Look up the WindowSlot this view belongs to. Two-step:
    /// view.handle → window.handle (VIEW_TO_WINDOW) → WindowSlot
    /// (WINDOWS). Returns `none` if the view isn't registered.
    windowSlot(): WindowSlot? {
        if let some(winH) = VIEW_TO_WINDOW.get(this.handle) {
            return WINDOWS.get(winH)
        }
        none
    }

    /// `kind`: 0=move, 1=down, 2=up. `button`: W3C MouseEvent.button.
    dispatchMouse(event: NSEvent, kind: i32, button: i32) {
        if let some(slot) = this.windowSlot() {
            let p = this.convertPointFromView(event.locationInWindow, nil)
            let w = this.frame.size.width
            let h = this.frame.size.height
            // `setAcceptsMouseMovedEvents(true)` makes the window
            // receive cursor motion globally while it's key — even
            // outside its own frame. Clip to the view's bounds here
            // so callers only see in-content events.
            if p.x < 0.0 || p.y < 0.0 || p.x > w || p.y > h { return }
            // GuiView overrides `isFlipped` to true, so `p.y` is
            // already in the W3C / DOM top-left convention — pass it
            // through unchanged.
            let flags = event.modifierFlags as u64
            let cb = match kind {
                1 { slot.onMouseDown }
                2 { slot.onMouseUp }
                _ { slot.onMouseMove }
            }
            cb(p.x, p.y, button, modCtrl(flags), modShift(flags),
               modAlt(flags), modMeta(flags))
        }
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
