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
    NSObject, NSApplication, NSWindow, NSView, NSButton, NSTextField,
    NSSlider, NSProgressIndicator, NSPopUpButton,
    NSScrollView, NSTableView, NSTableColumn, NSBox,
    NSProgressIndicatorStyle,
    NSNotification, NSEvent,
    NSRect, NSWindowStyleMask, NSApplicationActivationPolicy,
    NSEventModifierFlags, NSString,
    nsstr, ilstr, makeRect, makeSize,
    makeLabel, makeTextField, makeSlider,
    makeProgressBar, makeScrollView, selectorPtr, nil,
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
/// Linear search by Objective-C identity against `keys`. Uses
/// `[NSObject isEqual:]` rather than ilang's `==` — Cocoa
/// property getters that return `NSObject` (e.g.
/// `[NSNotification object]`) hand back a fresh ilang wrapper
/// around the same ObjC pointer on each call, so the
/// heap-pointer `==` compares wrapper identity and misses.
/// `isEqual:` defaults to pointer equality on the ObjC side,
/// which is what we want.
fn indexBy(keys: NSObject[], o: NSObject): i64 {
    let n = keys.length
    let i: i64 = 0
    let found: i64 = -1
    while i < n {
        if keys[i].isEqual(o) {
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

    // Created HIDDEN. The user controls first-paint via
    // `Window.show()` so they can mutate title / size / add
    // children before the window flashes on screen.

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

pub fn setWindowTitle(handle: i64, title: string) {
    if handle >= 0 && handle < (WINDOWS.length as i64) {
        WINDOWS[handle].setTitle(nsstr(title))
    }
}
pub fn setWindowSize(handle: i64, width: f64, height: f64) {
    if handle >= 0 && handle < (WINDOWS.length as i64) {
        WINDOWS[handle].setContentSize(makeSize(width, height))
    }
}
pub fn showNativeWindow(handle: i64) {
    if handle >= 0 && handle < (WINDOWS.length as i64) {
        WINDOWS[handle].makeKeyAndOrderFront(nil)
        let app = NSApplication.sharedApplication()
        app.activate()
    }
}
pub fn hideNativeWindow(handle: i64) {
    if handle >= 0 && handle < (WINDOWS.length as i64) {
        WINDOWS[handle].orderOut(nil)
    }
}

// ─── buttons ──────────────────────────────────────────────────
//
// One `ClickHandler` instance is registered for all buttons; on
// click the sender NSButton is looked up in `BUTTON_KEYS` and the
// matching slot's `ON_CLICK` callback fires. Each callback is
// (x, y, button) — gui-core wraps it into a MouseEvent.

let BUTTONS:    NSButton[]                     = []
let BUTTON_KEYS: NSObject[]                    = []
let ON_CLICK:   fn(f64, f64, i32)[]            = []
let CLICK_HANDLERS: NSObject[]                 = []   // ClickHandler instances

fn noOpClick(x: f64, y: f64, b: i32) {
    let _ = x
    let _ = y
    let _ = b
}

class ClickHandler : NSObject {
    pub clicked(sender: NSObject) {
        let ix = indexBy(BUTTON_KEYS, sender)
        if ix >= 0 {
            let cb = ON_CLICK[ix]
            // Cocoa target/action carries no cursor position; pass
            // the button's centre so the MouseEvent has something
            // sensible. Win32 BN_CLICKED is the same — no cursor.
            let btn = BUTTONS[ix]
            let f = btn.frame
            cb(f.origin.x + f.size.width / 2.0,
               f.origin.y + f.size.height / 2.0,
               0)
        }
    }
}

/// Allocate and attach an NSButton as a subview of `windowHandle`'s
/// content view. Returns the button's handle (= parallel-array
/// index). Caller must invoke `setButton*` to register the click
/// callback and to mutate `text` / `frame` / `enabled` / `hidden`.
pub fn createNativeButton(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (WINDOWS.length as i64) {
        return -1
    }
    let win = WINDOWS[windowHandle]
    let btn = NSButton.alloc().init(makeRect(x, y, width, height))
    btn.setTitle(nsstr(text))
    btn.setEnabled(enabled)
    btn.setHidden(!visible)

    ClickHandler.register()
    let h = ClickHandler.alloc().init()
    btn.setTarget(h)
    btn.setAction(selectorPtr("clicked:"))

    // NSButton inherits from NSView; addSubview accepts NSView.
    win.contentView.addSubview(btn as NSView)

    let handle: i64 = BUTTONS.length as i64
    BUTTONS.push(btn)
    BUTTON_KEYS.push(btn as NSObject)
    ON_CLICK.push(noOpClick)
    CLICK_HANDLERS.push(h as NSObject)
    handle
}

pub fn setOnButtonClick(handle: i64, cb: fn(f64, f64, i32)) {
    if handle >= 0 && handle < (ON_CLICK.length as i64) { ON_CLICK[handle] = cb }
}

pub fn setButtonText(handle: i64, text: string) {
    if handle >= 0 && handle < (BUTTONS.length as i64) {
        BUTTONS[handle].setTitle(nsstr(text))
    }
}
pub fn setButtonFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (BUTTONS.length as i64) {
        BUTTONS[handle].setFrame(makeRect(x, y, width, height))
    }
}
pub fn setButtonEnabled(handle: i64, enabled: bool) {
    if handle >= 0 && handle < (BUTTONS.length as i64) {
        BUTTONS[handle].setEnabled(enabled)
    }
}
pub fn setButtonVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (BUTTONS.length as i64) {
        BUTTONS[handle].setHidden(!visible)
    }
}

// ─── labels ───────────────────────────────────────────────────
//
// `makeLabel` flips off editable / bordered / bezeled / drawsBackground
// on an NSTextField so it reads as plain static text.

let LABELS: NSTextField[] = []

pub fn createNativeLabel(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (WINDOWS.length as i64) {
        return -1
    }
    let win = WINDOWS[windowHandle]
    let lbl = makeLabel(text, x, y, width, height)
    lbl.setHidden(!visible)
    win.contentView.addSubview(lbl as NSView)
    let handle: i64 = LABELS.length as i64
    LABELS.push(lbl)
    handle
}

pub fn setLabelText(handle: i64, text: string) {
    if handle >= 0 && handle < (LABELS.length as i64) {
        LABELS[handle].stringValue = nsstr(text)
    }
}
pub fn setLabelFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (LABELS.length as i64) {
        LABELS[handle].setFrame(makeRect(x, y, width, height))
    }
}
pub fn setLabelVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (LABELS.length as i64) {
        LABELS[handle].setHidden(!visible)
    }
}

// ─── text fields ──────────────────────────────────────────────
//
// One `ChangeHandler` is set as each NSTextField's delegate; on
// `controlTextDidChange:` it looks up the sender's slot in
// `EDIT_KEYS` and fires the registered change callback with the
// current contents.

let EDITS:           NSTextField[]      = []
let EDIT_KEYS:       NSObject[]         = []
let ON_TEXT_CHANGE:  fn(string)[]       = []
let CHANGE_HANDLERS: NSObject[]         = []

fn noOpText(s: string) { let _ = s }

class ChangeHandler : NSObject {
    pub controlTextDidChange(n: NSNotification) {
        let ix = indexBy(EDIT_KEYS, n.notificationObject)
        if ix >= 0 {
            let cb = ON_TEXT_CHANGE[ix]
            cb(ilstr(EDITS[ix].stringValue))
        }
    }
}

pub fn createNativeEdit(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (WINDOWS.length as i64) {
        return -1
    }
    let win = WINDOWS[windowHandle]
    let tf = makeTextField(text, x, y, width, height)
    tf.setEnabled(enabled)
    tf.setHidden(!visible)

    ChangeHandler.register()
    let h = ChangeHandler.alloc().init()
    tf.setDelegate(h as NSObject)

    win.contentView.addSubview(tf as NSView)

    let handle: i64 = EDITS.length as i64
    EDITS.push(tf)
    EDIT_KEYS.push(tf as NSObject)
    ON_TEXT_CHANGE.push(noOpText)
    CHANGE_HANDLERS.push(h as NSObject)
    handle
}

pub fn readEditText(handle: i64): string {
    if handle < 0 || handle >= (EDITS.length as i64) { return "" }
    ilstr(EDITS[handle].stringValue)
}

pub fn setOnEditChange(handle: i64, cb: fn(string)) {
    if handle >= 0 && handle < (ON_TEXT_CHANGE.length as i64) {
        ON_TEXT_CHANGE[handle] = cb
    }
}
pub fn setEditText(handle: i64, text: string) {
    if handle >= 0 && handle < (EDITS.length as i64) {
        EDITS[handle].stringValue = nsstr(text)
    }
}
pub fn setEditFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (EDITS.length as i64) {
        EDITS[handle].setFrame(makeRect(x, y, width, height))
    }
}
pub fn setEditEnabled(handle: i64, enabled: bool) {
    if handle >= 0 && handle < (EDITS.length as i64) {
        EDITS[handle].setEnabled(enabled)
    }
}
pub fn setEditVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (EDITS.length as i64) {
        EDITS[handle].setHidden(!visible)
    }
}

// ─── checkboxes (NSButton + setButtonType:switch) ────────────

let CHECKBOXES:        NSButton[]    = []
let CHECKBOX_KEYS:     NSObject[]    = []
let ON_TOGGLE:         fn(bool)[]    = []
let TOGGLE_HANDLERS:   NSObject[]    = []

fn noOpBool(b: bool) { let _ = b }

class ToggleHandler : NSObject {
    pub toggled(sender: NSObject) {
        let ix = indexBy(CHECKBOX_KEYS, sender)
        if ix >= 0 {
            let cb = ON_TOGGLE[ix]
            // NSControlStateValueOn = 1.
            cb(CHECKBOXES[ix].state == 1 as i64)
        }
    }
}

pub fn createNativeCheckbox(
    windowHandle: i64,
    text: string, checked: bool,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (WINDOWS.length as i64) {
        return -1
    }
    let win = WINDOWS[windowHandle]
    let cb = NSButton.alloc().init(makeRect(x, y, width, height))
    // NSButtonTypeSwitch = 3 — the checkbox look.
    cb.setButtonType(3 as i64)
    cb.setTitle(nsstr(text))
    cb.state = if checked { 1 as i64 } else { 0 as i64 }
    cb.setEnabled(enabled)
    cb.setHidden(!visible)

    ToggleHandler.register()
    let h = ToggleHandler.alloc().init()
    cb.setTarget(h)
    cb.setAction(selectorPtr("toggled:"))

    win.contentView.addSubview(cb as NSView)

    let handle: i64 = CHECKBOXES.length as i64
    CHECKBOXES.push(cb)
    CHECKBOX_KEYS.push(cb as NSObject)
    ON_TOGGLE.push(noOpBool)
    TOGGLE_HANDLERS.push(h as NSObject)
    handle
}

pub fn setOnCheckboxToggle(handle: i64, cb: fn(bool)) {
    if handle >= 0 && handle < (ON_TOGGLE.length as i64) {
        ON_TOGGLE[handle] = cb
    }
}
pub fn setCheckboxText(handle: i64, text: string) {
    if handle >= 0 && handle < (CHECKBOXES.length as i64) {
        CHECKBOXES[handle].setTitle(nsstr(text))
    }
}
pub fn setCheckboxChecked(handle: i64, checked: bool) {
    if handle >= 0 && handle < (CHECKBOXES.length as i64) {
        CHECKBOXES[handle].state = if checked { 1 as i64 } else { 0 as i64 }
    }
}
pub fn readCheckboxChecked(handle: i64): bool {
    if handle < 0 || handle >= (CHECKBOXES.length as i64) { return false }
    CHECKBOXES[handle].state == 1 as i64
}
pub fn setCheckboxFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (CHECKBOXES.length as i64) {
        CHECKBOXES[handle].setFrame(makeRect(x, y, width, height))
    }
}
pub fn setCheckboxEnabled(handle: i64, enabled: bool) {
    if handle >= 0 && handle < (CHECKBOXES.length as i64) {
        CHECKBOXES[handle].setEnabled(enabled)
    }
}
pub fn setCheckboxVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (CHECKBOXES.length as i64) {
        CHECKBOXES[handle].setHidden(!visible)
    }
}

// ─── sliders ─────────────────────────────────────────────────

let SLIDERS:         NSSlider[]    = []
let SLIDER_KEYS:     NSObject[]    = []
let ON_SLIDER:       fn(f64)[]     = []
let SLIDER_HANDLERS: NSObject[]    = []

fn noOpF64(v: f64) { let _ = v }

class SliderHandler : NSObject {
    pub slid(sender: NSObject) {
        let ix = indexBy(SLIDER_KEYS, sender)
        if ix >= 0 {
            let cb = ON_SLIDER[ix]
            cb(SLIDERS[ix].doubleValue)
        }
    }
}

pub fn createNativeSlider(
    windowHandle: i64,
    min: f64, max: f64, value: f64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (WINDOWS.length as i64) {
        return -1
    }
    let win = WINDOWS[windowHandle]
    let s = makeSlider(min, max, value, x, y, width, height)
    s.setContinuous(true)
    s.setHidden(!visible)
    let _ = enabled   // NSSlider has no setEnabled in our binding;
                      // surface omitted on macOS for now.

    SliderHandler.register()
    let h = SliderHandler.alloc().init()
    s.setTarget(h)
    s.setAction(selectorPtr("slid:"))

    win.contentView.addSubview(s as NSView)

    let handle: i64 = SLIDERS.length as i64
    SLIDERS.push(s)
    SLIDER_KEYS.push(s as NSObject)
    ON_SLIDER.push(noOpF64)
    SLIDER_HANDLERS.push(h as NSObject)
    handle
}

pub fn setOnSliderChange(handle: i64, cb: fn(f64)) {
    if handle >= 0 && handle < (ON_SLIDER.length as i64) {
        ON_SLIDER[handle] = cb
    }
}
pub fn setSliderRange(handle: i64, min: f64, max: f64) {
    if handle >= 0 && handle < (SLIDERS.length as i64) {
        SLIDERS[handle].setMinValue(min)
        SLIDERS[handle].setMaxValue(max)
    }
}
pub fn setSliderValue(handle: i64, value: f64) {
    if handle >= 0 && handle < (SLIDERS.length as i64) {
        SLIDERS[handle].doubleValue = value
    }
}
pub fn readSliderValue(handle: i64): f64 {
    if handle < 0 || handle >= (SLIDERS.length as i64) { return 0.0 }
    SLIDERS[handle].doubleValue
}
pub fn setSliderFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (SLIDERS.length as i64) {
        SLIDERS[handle].setFrame(makeRect(x, y, width, height))
    }
}
pub fn setSliderEnabled(handle: i64, enabled: bool) {
    // NSSlider inherits setEnabled from NSControl; the binding
    // doesn't expose it today, so this is a no-op on macOS.
    let _ = handle
    let _ = enabled
}
pub fn setSliderVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (SLIDERS.length as i64) {
        SLIDERS[handle].setHidden(!visible)
    }
}

// ─── progress bars ───────────────────────────────────────────

let PROGRESS_BARS: NSProgressIndicator[] = []

pub fn createNativeProgress(
    windowHandle: i64,
    min: f64, max: f64, value: f64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (WINDOWS.length as i64) {
        return -1
    }
    let win = WINDOWS[windowHandle]
    let p = makeProgressBar(min, max, value, x, y, width, height)
    p.setHidden(!visible)
    win.contentView.addSubview(p as NSView)
    let handle: i64 = PROGRESS_BARS.length as i64
    PROGRESS_BARS.push(p)
    handle
}

pub fn setProgressRange(handle: i64, min: f64, max: f64) {
    if handle >= 0 && handle < (PROGRESS_BARS.length as i64) {
        PROGRESS_BARS[handle].setMinValue(min)
        PROGRESS_BARS[handle].setMaxValue(max)
    }
}
pub fn setProgressValue(handle: i64, value: f64) {
    if handle >= 0 && handle < (PROGRESS_BARS.length as i64) {
        PROGRESS_BARS[handle].doubleValue = value
    }
}
pub fn setProgressFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (PROGRESS_BARS.length as i64) {
        PROGRESS_BARS[handle].setFrame(makeRect(x, y, width, height))
    }
}
pub fn setProgressVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (PROGRESS_BARS.length as i64) {
        PROGRESS_BARS[handle].setHidden(!visible)
    }
}

// ─── combo boxes (NSPopUpButton) ─────────────────────────────

let COMBOS:         NSPopUpButton[] = []
let COMBO_KEYS:     NSObject[]      = []
let ON_COMBO:       fn(i32)[]       = []
let COMBO_HANDLERS: NSObject[]      = []

fn noOpI32(i: i32) { let _ = i }

class ComboHandler : NSObject {
    pub picked(sender: NSObject) {
        let ix = indexBy(COMBO_KEYS, sender)
        if ix >= 0 {
            let cb = ON_COMBO[ix]
            cb(COMBOS[ix].indexOfSelectedItem as i32)
        }
    }
}

pub fn createNativeCombo(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (WINDOWS.length as i64) {
        return -1
    }
    let win = WINDOWS[windowHandle]
    let p = NSPopUpButton.alloc().init(makeRect(x, y, width, height), false)
    p.setEnabled(enabled)
    p.setHidden(!visible)

    ComboHandler.register()
    let h = ComboHandler.alloc().init()
    p.setTarget(h)
    p.setAction(selectorPtr("picked:"))

    win.contentView.addSubview(p as NSView)

    let handle: i64 = COMBOS.length as i64
    COMBOS.push(p)
    COMBO_KEYS.push(p as NSObject)
    ON_COMBO.push(noOpI32)
    COMBO_HANDLERS.push(h as NSObject)
    handle
}

pub fn setOnComboSelect(handle: i64, cb: fn(i32)) {
    if handle >= 0 && handle < (ON_COMBO.length as i64) {
        ON_COMBO[handle] = cb
    }
}
pub fn addComboItem(handle: i64, item: string) {
    if handle >= 0 && handle < (COMBOS.length as i64) {
        COMBOS[handle].addItemWithTitle(nsstr(item))
    }
}
pub fn clearComboItems(handle: i64) {
    if handle >= 0 && handle < (COMBOS.length as i64) {
        COMBOS[handle].removeAllItems()
    }
}
pub fn setComboSelectedIndex(handle: i64, index: i32) {
    if handle >= 0 && handle < (COMBOS.length as i64) {
        COMBOS[handle].selectItemAtIndex(index as i64)
    }
}
pub fn readComboSelectedIndex(handle: i64): i32 {
    if handle < 0 || handle >= (COMBOS.length as i64) { return -1 as i32 }
    COMBOS[handle].indexOfSelectedItem as i32
}
pub fn setComboFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (COMBOS.length as i64) {
        COMBOS[handle].setFrame(makeRect(x, y, width, height))
    }
}
pub fn setComboEnabled(handle: i64, enabled: bool) {
    if handle >= 0 && handle < (COMBOS.length as i64) {
        COMBOS[handle].setEnabled(enabled)
    }
}
pub fn setComboVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (COMBOS.length as i64) {
        COMBOS[handle].setHidden(!visible)
    }
}

// ─── radio buttons (NSButton + setButtonType:radio) ──────────
//
// Mutual exclusion is driven by gui-core's per-Window
// `_unselectSiblings` walk, so this backend just surfaces the
// click and lets the core toggle siblings via `setRadioChecked`.

let RADIOS:           NSButton[] = []
let RADIO_KEYS:       NSObject[] = []
let ON_RADIO:         fn(bool)[] = []
let RADIO_HANDLERS:   NSObject[] = []

class RadioHandler : NSObject {
    pub radioClicked(sender: NSObject) {
        let ix = indexBy(RADIO_KEYS, sender)
        if ix >= 0 {
            let cb = ON_RADIO[ix]
            cb(RADIOS[ix].state == 1 as i64)
        }
    }
}

pub fn createNativeRadio(
    windowHandle: i64,
    text: string, checked: bool,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (WINDOWS.length as i64) {
        return -1
    }
    let win = WINDOWS[windowHandle]
    let r = NSButton.alloc().init(makeRect(x, y, width, height))
    // NSButtonTypeRadio = 4.
    r.setButtonType(4 as i64)
    r.setTitle(nsstr(text))
    r.state = if checked { 1 as i64 } else { 0 as i64 }
    r.setEnabled(enabled)
    r.setHidden(!visible)

    RadioHandler.register()
    let h = RadioHandler.alloc().init()
    r.setTarget(h)
    r.setAction(selectorPtr("radioClicked:"))

    win.contentView.addSubview(r as NSView)

    let handle: i64 = RADIOS.length as i64
    RADIOS.push(r)
    RADIO_KEYS.push(r as NSObject)
    ON_RADIO.push(noOpBool)
    RADIO_HANDLERS.push(h as NSObject)
    handle
}

pub fn setOnRadioToggle(handle: i64, cb: fn(bool)) {
    if handle >= 0 && handle < (ON_RADIO.length as i64) {
        ON_RADIO[handle] = cb
    }
}
pub fn setRadioText(handle: i64, text: string) {
    if handle >= 0 && handle < (RADIOS.length as i64) {
        RADIOS[handle].setTitle(nsstr(text))
    }
}
pub fn setRadioChecked(handle: i64, checked: bool) {
    if handle >= 0 && handle < (RADIOS.length as i64) {
        RADIOS[handle].state = if checked { 1 as i64 } else { 0 as i64 }
    }
}
pub fn readRadioChecked(handle: i64): bool {
    if handle < 0 || handle >= (RADIOS.length as i64) { return false }
    RADIOS[handle].state == 1 as i64
}
pub fn setRadioFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (RADIOS.length as i64) {
        RADIOS[handle].setFrame(makeRect(x, y, width, height))
    }
}
pub fn setRadioEnabled(handle: i64, enabled: bool) {
    if handle >= 0 && handle < (RADIOS.length as i64) {
        RADIOS[handle].setEnabled(enabled)
    }
}
pub fn setRadioVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (RADIOS.length as i64) {
        RADIOS[handle].setHidden(!visible)
    }
}

// ─── list boxes (NSTableView in NSScrollView) ────────────────
//
// Single-column read-only list. Each slot owns a `ListDataSource`
// that the table queries for row count + cell value; the
// `tableViewSelectionDidChange:` delegate hop posts the new
// `selectedRow` back to the registered callback.

let LISTS:            NSTableView[]   = []
let LIST_SCROLLS:     NSScrollView[]  = []
let LIST_KEYS:        NSObject[]      = []   // keyed by NSTableView identity
let LIST_DS:          NSObject[]      = []   // data source instances (parallel)
let LIST_ITEMS:       string[][]      = []   // items per slot
let ON_LIST:          fn(i32)[]       = []

class ListDataSource : NSObject {
    pub numberOfRowsInTableView(t: NSTableView): i64 {
        let _ = t
        let ix = indexBy(LIST_DS, this as NSObject)
        if ix < 0 { return 0 }
        LIST_ITEMS[ix].length
    }

    pub tableViewObjectValueForTableColumn(
        t: NSTableView, column: NSTableColumn, row: i64
    ): NSObject {
        let _ = t
        let _ = column
        let ix = indexBy(LIST_DS, this as NSObject)
        if ix < 0 { return nsstr("") as NSObject }
        let items = LIST_ITEMS[ix]
        if row < 0 || row >= items.length { return nsstr("") as NSObject }
        nsstr(items[row]) as NSObject
    }
}

class ListDelegate : NSObject {
    pub tableViewSelectionDidChange(n: NSNotification) {
        let table = n.notificationObject
        let ix = indexBy(LIST_KEYS, table)
        if ix >= 0 {
            let cb = ON_LIST[ix]
            cb(LISTS[ix].selectedRow as i32)
        }
    }
}

let LIST_DELEGATES: NSObject[] = []

pub fn createNativeList(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (WINDOWS.length as i64) {
        return -1
    }
    let _ = enabled   // NSTableView selection isn't gated by an
                      // enabled flag in the same way; surfaced as
                      // no-op so the gui-core call site stays
                      // platform-uniform.
    let win = WINDOWS[windowHandle]
    let table = NSTableView.alloc().init(makeRect(0.0, 0.0, width, height))
    // Single anonymous column — width matches the scroll-view
    // contents so the column spans the visible area.
    let col = NSTableColumn.alloc().initWithIdentifier(nsstr("v"))
    table.addTableColumn(col)
    table.setHeaderView(nil as NSView)
    table.setAllowsMultipleSelection(false)

    ListDataSource.register()
    let ds = ListDataSource.alloc().init()
    table.setDataSource(ds as NSObject)

    ListDelegate.register()
    let dg = ListDelegate.alloc().init()
    table.setDelegate(dg as NSObject)

    let scroll = makeScrollView(table as NSView, x, y, width, height)
    scroll.setHidden(!visible)
    win.contentView.addSubview(scroll as NSView)

    let handle: i64 = LISTS.length as i64
    LISTS.push(table)
    LIST_SCROLLS.push(scroll)
    LIST_KEYS.push(table as NSObject)
    LIST_DS.push(ds as NSObject)
    LIST_ITEMS.push([])
    ON_LIST.push(noOpI32)
    LIST_DELEGATES.push(dg as NSObject)
    handle
}

pub fn setOnListSelect(handle: i64, cb: fn(i32)) {
    if handle >= 0 && handle < (ON_LIST.length as i64) {
        ON_LIST[handle] = cb
    }
}
pub fn addListItem(handle: i64, item: string) {
    if handle >= 0 && handle < (LIST_ITEMS.length as i64) {
        LIST_ITEMS[handle].push(item)
        LISTS[handle].reloadData()
    }
}
pub fn clearListItems(handle: i64) {
    if handle >= 0 && handle < (LIST_ITEMS.length as i64) {
        LIST_ITEMS[handle] = []
        LISTS[handle].reloadData()
    }
}
pub fn setListSelectedIndex(handle: i64, index: i32) {
    if handle >= 0 && handle < (LISTS.length as i64) {
        if index < 0 {
            LISTS[handle].deselectAll(nil)
        } else {
            // NSTableView wants an NSIndexSet; the cocoa bindings
            // don't expose a single-row constructor today, so we
            // fall back to selecting nothing and letting the user
            // re-pick. Programmatic selection is uncommon enough
            // that this gap is acceptable on the macOS side.
            let _ = index
        }
    }
}
pub fn readListSelectedIndex(handle: i64): i32 {
    if handle < 0 || handle >= (LISTS.length as i64) { return -1 as i32 }
    LISTS[handle].selectedRow as i32
}
pub fn setListFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (LIST_SCROLLS.length as i64) {
        LIST_SCROLLS[handle].setFrame(makeRect(x, y, width, height))
    }
}
pub fn setListEnabled(handle: i64, enabled: bool) {
    // No direct NSTableView enabled toggle; surface as no-op.
    let _ = handle
    let _ = enabled
}
pub fn setListVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (LIST_SCROLLS.length as i64) {
        LIST_SCROLLS[handle].setHidden(!visible)
    }
}

// ─── group boxes (NSBox) ─────────────────────────────────────

let GROUPBOXES: NSBox[] = []

pub fn createNativeGroupBox(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (WINDOWS.length as i64) {
        return -1
    }
    let win = WINDOWS[windowHandle]
    let box = NSBox.alloc().init(makeRect(x, y, width, height))
    box.setTitle(nsstr(text))
    box.setHidden(!visible)
    win.contentView.addSubview(box as NSView)
    let handle: i64 = GROUPBOXES.length as i64
    GROUPBOXES.push(box)
    handle
}

pub fn setGroupBoxText(handle: i64, text: string) {
    if handle >= 0 && handle < (GROUPBOXES.length as i64) {
        GROUPBOXES[handle].setTitle(nsstr(text))
    }
}
pub fn setGroupBoxFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (GROUPBOXES.length as i64) {
        GROUPBOXES[handle].setFrame(makeRect(x, y, width, height))
    }
}
pub fn setGroupBoxVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (GROUPBOXES.length as i64) {
        GROUPBOXES[handle].setHidden(!visible)
    }
}

pub fn runEventLoop() {
    let app = NSApplication.sharedApplication()
    app.run()
}
