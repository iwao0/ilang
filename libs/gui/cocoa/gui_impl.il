/// macOS implementation of the `gui-core` bridge API.
///
/// `gui-core` (consumer-facing package) calls the functions in
/// this file via `[deps] @target("macos")`. Everything Cocoa-
/// specific stays here — gui-core never sees `NSWindow`,
/// `NSObject`, or any `objc_msgSend`. Public API contract (must
/// stay in sync with `libs/gui/win32`):
///
///   createNativeWindow(title, w, h): i64
///   setOnClose / setOnResize / setOnKeyDown / setOnKeyUp /
///     setOnMouseMove / setOnMouseDown / setOnMouseUp(handle, cb)
///   runEventLoop()
///
/// The `handle` returned to gui-core is the index into the
/// parallel arrays below — we can't cast an `NSObject` value to
/// `i64` (the type system refuses it for non-`@handle` classes),
/// so an array slot is the simplest stable per-window key.

use cocoa {
    NSObject, NSApplication, NSWindow, NSView, NSNotification, NSEvent,
    NSWindowStyleMask, NSApplicationActivationPolicy,
    NSEventModifierFlags, nil,
    sharedApplication, makeWindow,
}

// ─── per-window state ─────────────────────────────────────────

let WINDOWS:        NSWindow[] = []
let WIN_KEYS:       NSObject[] = []   // parallel to WINDOWS, keyed by ==
let DELEGATES:      NSObject[] = []
let VIEWS:          NSObject[] = []
let ON_CLOSE:       fn()[]                                                  = []
let ON_RESIZE:      fn(f64, f64)[]                                          = []
let ON_KEY_DOWN:    fn(string, bool, bool, bool, bool)[]                    = []
let ON_KEY_UP:      fn(string, bool, bool, bool, bool)[]                    = []
let ON_MOUSE_MOVE:  fn(f64, f64, i32, bool, bool, bool, bool)[]             = []
let ON_MOUSE_DOWN:  fn(f64, f64, i32, bool, bool, bool, bool)[]             = []
let ON_MOUSE_UP:    fn(f64, f64, i32, bool, bool, bool, bool)[]             = []

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

/// Locate the WINDOWS slot for `w`. NSObject `==` is pointer
/// equality which is exactly what we want here. Returns -1 when
/// the window is no longer in the table (e.g. dispatch came in
/// after teardown).
/// Linear search by NSObject pointer equality against `keys`.
/// Both `WINDOWS` (matched on `windowWillClose` / `windowDidResize`
/// notifications) and `VIEWS` (matched on keyDown / mouseDown
/// inside the GuiView) use this. Returns -1 when not found.
fn indexBy(keys: NSObject[], o: NSObject): i64 {
    let n = keys.length
    let i: i64 = 0
    let found: i64 = -1
    while i < n {
        if keys[i] == o {
            found = i
            i = n
        } else {
            i = i + 1
        }
    }
    found
}

// ─── modifier-flag helpers ────────────────────────────────────

fn modShift(flags: u64): bool { (flags & (NSEventModifierFlags.shift   as u64)) != 0 }
fn modCtrl(flags:  u64): bool { (flags & (NSEventModifierFlags.control as u64)) != 0 }
fn modAlt(flags:   u64): bool { (flags & (NSEventModifierFlags.option  as u64)) != 0 }
fn modMeta(flags:  u64): bool { (flags & (NSEventModifierFlags.command as u64)) != 0 }

// ─── keyCode → W3C KeyboardEvent.code ─────────────────────────
//
// Cocoa virtual key codes are in `<HIToolbox/Events.h>`. Only the
// keys with a standard W3C `code` are translated; unknowns
// become an empty string so user code can filter.

fn keyCodeToString(k: u16): string {
    match k {
        0   { "KeyA" }
        1   { "KeyS" }
        2   { "KeyD" }
        3   { "KeyF" }
        4   { "KeyH" }
        5   { "KeyG" }
        6   { "KeyZ" }
        7   { "KeyX" }
        8   { "KeyC" }
        9   { "KeyV" }
        11  { "KeyB" }
        12  { "KeyQ" }
        13  { "KeyW" }
        14  { "KeyE" }
        15  { "KeyR" }
        16  { "KeyY" }
        17  { "KeyT" }
        18  { "Digit1" }
        19  { "Digit2" }
        20  { "Digit3" }
        21  { "Digit4" }
        22  { "Digit6" }
        23  { "Digit5" }
        25  { "Digit9" }
        26  { "Digit7" }
        28  { "Digit8" }
        29  { "Digit0" }
        31  { "KeyO" }
        32  { "KeyU" }
        34  { "KeyI" }
        35  { "KeyP" }
        36  { "Enter" }
        37  { "KeyL" }
        38  { "KeyJ" }
        40  { "KeyK" }
        45  { "KeyN" }
        46  { "KeyM" }
        48  { "Tab" }
        49  { "Space" }
        51  { "Backspace" }
        53  { "Escape" }
        123 { "ArrowLeft" }
        124 { "ArrowRight" }
        125 { "ArrowDown" }
        126 { "ArrowUp" }
        _   { "" }
    }
}

// ─── window delegate (close + resize) ─────────────────────────

class GuiDelegate : NSObject {
    pub windowWillClose(n: NSNotification) {
        let ix = indexBy(WIN_KEYS, n.notificationObject)
        if ix >= 0 {
            let cb = ON_CLOSE[ix]
            cb()
        }
        let app = NSApplication.sharedApplication()
        app.terminate(nil)
    }

    pub windowDidResize(n: NSNotification) {
        let ix = indexBy(WIN_KEYS, n.notificationObject)
        if ix >= 0 {
            let win = WINDOWS[ix]
            let frame = win.contentView.frame
            let cb = ON_RESIZE[ix]
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

class GuiView : NSView {
    pub override acceptsFirstResponder(): bool { true }

    pub override keyDown(event: NSEvent) {
        let ix = indexBy(VIEWS, this as NSObject)
        if ix >= 0 {
            let cb = ON_KEY_DOWN[ix]
            let flags = event.modifierFlags as u64
            cb(keyCodeToString(event.keyCode), modCtrl(flags), modShift(flags),
               modAlt(flags), modMeta(flags))
        }
    }

    pub override keyUp(event: NSEvent) {
        let ix = indexBy(VIEWS, this as NSObject)
        if ix >= 0 {
            let cb = ON_KEY_UP[ix]
            let flags = event.modifierFlags as u64
            cb(keyCodeToString(event.keyCode), modCtrl(flags), modShift(flags),
               modAlt(flags), modMeta(flags))
        }
    }

    pub override mouseMoved(event: NSEvent)     { this.dispatchMouse(event, ON_MOUSE_MOVE, 0) }
    pub override mouseDragged(event: NSEvent)   { this.dispatchMouse(event, ON_MOUSE_MOVE, 0) }
    pub override mouseDown(event: NSEvent)      { this.dispatchMouse(event, ON_MOUSE_DOWN, 0) }
    pub override mouseUp(event: NSEvent)        { this.dispatchMouse(event, ON_MOUSE_UP,   0) }
    pub override rightMouseDown(event: NSEvent) { this.dispatchMouse(event, ON_MOUSE_DOWN, 2) }
    pub override rightMouseUp(event: NSEvent)   { this.dispatchMouse(event, ON_MOUSE_UP,   2) }
    pub override otherMouseDown(event: NSEvent) { this.dispatchMouse(event, ON_MOUSE_DOWN, 1) }
    pub override otherMouseUp(event: NSEvent)   { this.dispatchMouse(event, ON_MOUSE_UP,   1) }

    dispatchMouse(
        event: NSEvent,
        bucket: fn(f64, f64, i32, bool, bool, bool, bool)[],
        button: i32
    ) {
        let ix = indexBy(VIEWS, this as NSObject)
        if ix < 0 { return }
        let p = this.convertPointFromView(event.locationInWindow, nil)
        let h = this.frame.size.height
        // Cocoa origin = bottom-left; flip Y so callers see the
        // W3C / DOM convention (top-left origin).
        let flags = event.modifierFlags as u64
        let cb = bucket[ix]
        cb(p.x, h - p.y, button, modCtrl(flags), modShift(flags),
           modAlt(flags), modMeta(flags))
    }
}

// ─── public API ────────────────────────────────────────────────

pub fn createNativeWindow(title: string, width: f64, height: f64): i64 {
    let app = sharedApplication()
    app.setActivationPolicy(NSApplicationActivationPolicy.regular)

    let mask = NSWindowStyleMask.titled
        | NSWindowStyleMask.closable
        | NSWindowStyleMask.miniaturizable
        | NSWindowStyleMask.resizable
    let win = makeWindow(title, 0.0, 0.0, width, height, mask)
    win.center()

    GuiDelegate.register()
    let d = GuiDelegate.alloc().init()
    win.setDelegate(d)

    GuiView.register()
    let v = GuiView.alloc().init()
    win.setContentView(v)
    win.setAcceptsMouseMovedEvents(true)
    win.makeFirstResponder(v)

    win.makeKeyAndOrderFront(nil)
    app.activate()

    let handle: i64 = WINDOWS.length as i64
    WINDOWS.push(win)
    WIN_KEYS.push(win as NSObject)
    DELEGATES.push(d as NSObject)
    VIEWS.push(v as NSObject)
    ON_CLOSE.push(noOp1)
    ON_RESIZE.push(noOp2)
    ON_KEY_DOWN.push(noOpKey)
    ON_KEY_UP.push(noOpKey)
    ON_MOUSE_MOVE.push(noOpMouse)
    ON_MOUSE_DOWN.push(noOpMouse)
    ON_MOUSE_UP.push(noOpMouse)
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

pub fn runEventLoop() {
    let app = NSApplication.sharedApplication()
    app.run()
}
