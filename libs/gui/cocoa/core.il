/// Shared per-window state plus the AppKit delegate / view
/// subclasses that catch close / resize / key / mouse events.
///
/// Every widget file (`button.il` / `label.il` / …) imports the
/// `WINDOWS` table from this module to find the parent NSWindow
/// the widget was created against.

use cocoa {
    NSObject, NSApplication, NSWindow, NSView, NSNotification, NSEvent,
    NSEventModifierFlags, KeyCode, nil,
}
use super.event

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

    pub init(win: NSWindow, delegate: NSObject, view: NSObject, viewHandle: i64) {
        this.win         = win
        this.delegate    = delegate
        this.view        = view
        this.viewHandle  = viewHandle
        this.onClose     = noOp1
        this.onResize    = noOp2
        this.onKeyDown   = noOpKey
        this.onKeyUp     = noOpKey
        this.onMouseMove = noOpMouse
        this.onMouseDown = noOpMouse
        this.onMouseUp   = noOpMouse
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
    let _ = w
    let _ = h
}
pub fn noOpKey(c: string, k1: bool, k2: bool, k3: bool, k4: bool) {
    let _ = c
    let _ = k1
    let _ = k2
    let _ = k3
    let _ = k4
}
pub fn noOpMouse(x: f64, y: f64, b: i32, k1: bool, k2: bool, k3: bool, k4: bool) {
    let _ = x
    let _ = y
    let _ = b
    let _ = k1
    let _ = k2
    let _ = k3
    let _ = k4
}
pub fn noOpClick(x: f64, y: f64, b: i32) {
    let _ = x
    let _ = y
    let _ = b
}
pub fn noOpBool(b: bool) { let _ = b }
pub fn noOpText(s: string) { let _ = s }
pub fn noOpF64(v: f64) { let _ = v }
pub fn noOpI32(i: i32) { let _ = i }

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
/// `event.KeyCode` (W3C `KeyboardEvent.code`). Unmapped keys
/// fall through to `event.KeyCode.unknown`; the caller then
/// serialises that into the discriminant string for the
/// cross-package callback ABI.
pub fn keyCodeToEnum(k: KeyCode): event.KeyCode {
    match k {
        KeyCode.keyA       { event.KeyCode.keyA }
        KeyCode.keyS       { event.KeyCode.keyS }
        KeyCode.keyD       { event.KeyCode.keyD }
        KeyCode.keyF       { event.KeyCode.keyF }
        KeyCode.keyH       { event.KeyCode.keyH }
        KeyCode.keyG       { event.KeyCode.keyG }
        KeyCode.keyZ       { event.KeyCode.keyZ }
        KeyCode.keyX       { event.KeyCode.keyX }
        KeyCode.keyC       { event.KeyCode.keyC }
        KeyCode.keyV       { event.KeyCode.keyV }
        KeyCode.keyB       { event.KeyCode.keyB }
        KeyCode.keyQ       { event.KeyCode.keyQ }
        KeyCode.keyW       { event.KeyCode.keyW }
        KeyCode.keyE       { event.KeyCode.keyE }
        KeyCode.keyR       { event.KeyCode.keyR }
        KeyCode.keyY       { event.KeyCode.keyY }
        KeyCode.keyT       { event.KeyCode.keyT }
        KeyCode.keyO       { event.KeyCode.keyO }
        KeyCode.keyU       { event.KeyCode.keyU }
        KeyCode.keyI       { event.KeyCode.keyI }
        KeyCode.keyP       { event.KeyCode.keyP }
        KeyCode.keyL       { event.KeyCode.keyL }
        KeyCode.keyJ       { event.KeyCode.keyJ }
        KeyCode.keyK       { event.KeyCode.keyK }
        KeyCode.keyN       { event.KeyCode.keyN }
        KeyCode.keyM       { event.KeyCode.keyM }
        KeyCode.key0       { event.KeyCode.digit0 }
        KeyCode.key1       { event.KeyCode.digit1 }
        KeyCode.key2       { event.KeyCode.digit2 }
        KeyCode.key3       { event.KeyCode.digit3 }
        KeyCode.key4       { event.KeyCode.digit4 }
        KeyCode.key5       { event.KeyCode.digit5 }
        KeyCode.key6       { event.KeyCode.digit6 }
        KeyCode.key7       { event.KeyCode.digit7 }
        KeyCode.key8       { event.KeyCode.digit8 }
        KeyCode.key9       { event.KeyCode.digit9 }
        KeyCode.returnKey  { event.KeyCode.enter }
        KeyCode.escape     { event.KeyCode.escape }
        KeyCode.delete     { event.KeyCode.backspace }
        KeyCode.tab        { event.KeyCode.tab }
        KeyCode.space      { event.KeyCode.space }
        KeyCode.leftArrow  { event.KeyCode.arrowLeft }
        KeyCode.rightArrow { event.KeyCode.arrowRight }
        KeyCode.downArrow  { event.KeyCode.arrowDown }
        KeyCode.upArrow    { event.KeyCode.arrowUp }
        _                  { event.KeyCode.unknown }
    }
}

// ─── window delegate (close + resize) ─────────────────────────

pub class GuiDelegate : NSObject {
    pub windowWillClose(n: NSNotification) {
        if let some(slot) = WINDOWS.get(n.notificationObject.handle) {
            let cb = slot.onClose
            cb()
        }
        let app = NSApplication.sharedApplication()
        app.terminate(nil)
    }

    pub windowDidResize(n: NSNotification) {
        if let some(slot) = WINDOWS.get(n.notificationObject.handle) {
            let frame = slot.win.contentView.frame
            let cb = slot.onResize
            cb(frame.size.width, frame.size.height)
        }
    }
}

// ─── content view (key + mouse) ───────────────────────────────
//
// NSWindow delegates don't see keyDown:/mouseDown: — those go
// through the responder chain to the firstResponder, normally
// the contentView. We replace contentView with a GuiView subclass
// that forwards each event to the per-window callback.

pub class GuiView : NSView {
    pub override acceptsFirstResponder(): bool { true }

    pub override keyDown(event: NSEvent) {
        if let some(slot) = this.windowSlot() {
            let cb = slot.onKeyDown
            let flags = event.modifierFlags as u64
            cb(keyCodeToString(event.keyCode), modCtrl(flags), modShift(flags),
               modAlt(flags), modMeta(flags))
        }
    }

    pub override keyUp(event: NSEvent) {
        if let some(slot) = this.windowSlot() {
            let cb = slot.onKeyUp
            let flags = event.modifierFlags as u64
            cb(keyCodeToString(event.keyCode), modCtrl(flags), modShift(flags),
               modAlt(flags), modMeta(flags))
        }
    }

    pub override mouseMoved(event: NSEvent)     { this.dispatchMouse(event, 0, 0) }
    pub override mouseDragged(event: NSEvent)   { this.dispatchMouse(event, 0, 0) }
    pub override mouseDown(event: NSEvent)      { this.dispatchMouse(event, 1, 0) }
    pub override mouseUp(event: NSEvent)        { this.dispatchMouse(event, 2, 0) }
    pub override rightMouseDown(event: NSEvent) { this.dispatchMouse(event, 1, 2) }
    pub override rightMouseUp(event: NSEvent)   { this.dispatchMouse(event, 2, 2) }
    pub override otherMouseDown(event: NSEvent) { this.dispatchMouse(event, 1, 1) }
    pub override otherMouseUp(event: NSEvent)   { this.dispatchMouse(event, 2, 1) }

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
            // Cocoa origin = bottom-left; flip Y so callers see the
            // W3C / DOM convention (top-left origin).
            let flags = event.modifierFlags as u64
            let cb = match kind {
                1 { slot.onMouseDown }
                2 { slot.onMouseUp }
                _ { slot.onMouseMove }
            }
            cb(p.x, h - p.y, button, modCtrl(flags), modShift(flags),
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
