/// Push button. NSButton subview of the parent window's
/// contentView. Click target/action goes through one shared
/// `ClickHandler` that looks the sender up in `BUTTONS` by its
/// raw ObjC pointer (`obj.handle`) — same field that gui-core
/// stores as the public `handle: i64`.

use cocoa {
    NSObject, NSButton, NSView, NSEvent,
    NSTrackingArea, NSTrackingAreaOptions,
    nsstr, makeRect, selectorPtr, nil,
    classPtr, setObjectClass,
}
use core {
    WINDOWS, noOpClick, noOp1, noOpKey,
    keyCodeToString, modCtrl, modShift, modAlt, modMeta,
}
use events { KeyCode }

/// Per-button state. Stored in `BUTTONS` keyed by the NSButton's
/// raw ObjC pointer (`btn.handle`). Same key gui-core sees as the
/// public handle, so both the public setters and the
/// click-handler-side sender lookup are O(1) Map gets.
class ButtonSlot {
    pub btn:     NSButton
    pub onClick: fn(f64, f64, i32)
    pub onFocus: fn()
    pub onBlur:  fn()
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseDown:  fn(f64, f64, i32)
    pub onMouseUp:    fn(f64, f64, i32)
    pub onContextMenu: fn(f64, f64, i32)
    pub onKeyDown:    fn(string, bool, bool, bool, bool)
    pub onKeyUp:      fn(string, bool, bool, bool, bool)
    pub handler: NSObject
    pub winHandle: i64
    pub init(btn: NSButton, handler: NSObject, winHandle: i64) {
        this.btn = btn
        this.onClick = noOpClick
        this.onFocus = noOp1
        this.onBlur  = noOp1
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseDown  = noOpClick
        this.onMouseUp    = noOpClick
        this.onContextMenu = noOpClick
        this.onKeyDown = noOpKey
        this.onKeyUp   = noOpKey
        this.handler = handler
        this.winHandle = winHandle
    }
}

fn noOpEnterLeave(x: f64, y: f64) {}

let BUTTONS = new Map<i64, ButtonSlot>()

/// Window handle → current default-button handle. AppKit only lets
/// one button per window carry `keyEquivalent = "\r"`; this map
/// tracks which one so a new `isDefault = true` setter can clear
/// the previous holder.
let DEFAULT_BUTTONS = new Map<i64, i64>()

class ClickHandler : NSObject {
    pub clicked(sender: NSObject) {
        if let some(slot) = BUTTONS.get(sender.handle) {
            let f = slot.btn.frame
            // Cocoa target/action carries no cursor position; pass
            // the button's centre so the MouseEvent has something
            // sensible.
            let cb = slot.onClick
            cb(f.origin.x + f.size.width / 2.0,
               f.origin.y + f.size.height / 2.0,
               0)
        }
    }
}

/// `NSButton` subclass used as an isa-swap target so AppKit's
/// `becomeFirstResponder` / `resignFirstResponder` dispatch lands
/// in our IMPs and we can surface `onFocus` / `onBlur`. Construction
/// stays on the unmodified `NSButton` path so `initWithFrame:`
/// initialises the cell properly; once the button is built we
/// rewrite its `isa` pointer to `FocusButton` (the KVO trick).
///
/// Methods are written without the `override` keyword because the
/// parent's bindings don't expose `becomeFirstResponder` /
/// `resignFirstResponder` — mirrors how `GuiView` declares
/// `acceptsFirstResponder` and `isFlipped`.
class FocusButton : NSButton {
    /// AppKit's default `NSButton.acceptsFirstResponder` returns
    /// false unless the user has Full Keyboard Access turned on in
    /// System Settings. Without this override `makeFirstResponder:`
    /// silently refuses to install us, and `keyDown:` never fires.
    pub acceptsFirstResponder(): bool { true }
    pub becomeFirstResponder(): bool {
        if let some(slot) = BUTTONS.get(this.handle) {
            let cb = slot.onFocus
            cb()
        }
        true
    }
    pub resignFirstResponder(): bool {
        if let some(slot) = BUTTONS.get(this.handle) {
            let cb = slot.onBlur
            cb()
        }
        true
    }
    /// AppKit dispatches `mouseDown:` here when the user presses
    /// the primary button inside the view. We hand the event to
    /// `super` first so `NSButton`'s default press handling
    /// (highlight cell, run target/action on a successful click,
    /// etc.) stays intact, then surface the event as
    /// `onMouseDown`. AppKit's own target/action drives `onClick`
    /// via `ClickHandler.clicked:`; we don't need to synthesise it.
    /// AppKit dispatches `mouseDown:` here when the user presses
    /// the primary button inside the view. We deliberately don't
    /// invoke `super.mouseDown(event)` — the ilang ObjC-super
    /// trampoline crashed with a SIGSEGV at `objc_msgSendSuper2`
    /// (FAR = 0x10, the `struct objc_super` size) on real AppKit
    /// dispatch. Instead we drive the press visual manually via
    /// `setHighlighted(true)` and synthesise the missing
    /// target/action on the matching `mouseUp:` while the cursor
    /// is still inside the bounds (DOM `click` semantics).
    pub override mouseDown(event: NSObject) {
        this.setHighlighted(true)
        // Wrap as `NSEvent` via the @objc desugar's unowned binder.
        // `event as? NSEvent` doesn't work — ilang's checked downcast
        // reads `obj+0` as the ilang class id, but an @objc handle
        // stores objc's isa pointer there, so the test always fails.
        // `__wrap_handle_unowned` takes the raw `id` we already
        // possess and rebinds it under the more-specific ilang type
        // *without* claiming ownership (AppKit autoreleases the event,
        // so an owning wrap would double-release on scope exit).
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = BUTTONS.get(this.handle) {
            let p = this.convertPointFromView(e.locationInWindow, nil)
            // NSView's `isFlipped` defaults to `false`; convert
            // AppKit's bottom-left origin to W3C top-left so the
            // value matches the existing onClick payload.
            let y = this.frame.size.height - p.y
            let cb = slot.onMouseDown
            cb(p.x, y, 0)
            PENDING_CLICK[this.handle] = true
        }
    }
    pub override mouseUp(event: NSObject) {
        this.setHighlighted(false)
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = BUTTONS.get(this.handle) {
            let f = this.frame
            let p = this.convertPointFromView(e.locationInWindow, nil)
            let y = f.size.height - p.y
            let upCb = slot.onMouseUp
            upCb(p.x, y, 0)
            let pending = match PENDING_CLICK.get(this.handle) {
                some(b) { b }
                none    { false }
            }
            PENDING_CLICK[this.handle] = false
            let inside = p.x >= 0.0 && p.y >= 0.0
                       && p.x <= f.size.width && p.y <= f.size.height
            if pending && inside {
                let clk = slot.onClick
                clk(f.origin.x + f.size.width / 2.0,
                    f.origin.y + f.size.height / 2.0,
                    0)
            }
        }
    }
    pub override mouseEntered(event: NSObject) {
        // No `super.mouseEntered` — same SIGSEGV issue with the
        // ilang ObjC-super trampoline. NSButton's stock
        // `mouseEntered:` doesn't drive a visible hover effect, so
        // omitting it costs no observable feedback.
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = BUTTONS.get(this.handle) {
            let p = this.convertPointFromView(e.locationInWindow, nil)
            let y = this.frame.size.height - p.y
            let cb = slot.onMouseEnter
            cb(p.x, y)
        }
    }
    pub override mouseExited(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = BUTTONS.get(this.handle) {
            let p = this.convertPointFromView(e.locationInWindow, nil)
            let y = this.frame.size.height - p.y
            let cb = slot.onMouseLeave
            cb(p.x, y)
        }
    }
    /// Secondary (right) button. AppKit routes it through its own
    /// selector pair — `mouseDown:` / `mouseUp:` cover the primary
    /// button only. Surface as `onMouseDown` / `onMouseUp` with
    /// `button = 2` (W3C). We deliberately don't drive
    /// `setHighlighted:` or synthesise `onClick` here: web `click`
    /// fires for primary only, and NSButton itself doesn't visually
    /// "press" on a right-click.
    pub override rightMouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = BUTTONS.get(this.handle) {
            let p = this.convertPointFromView(e.locationInWindow, nil)
            let y = this.frame.size.height - p.y
            let cb = slot.onMouseDown
            cb(p.x, y, 2)
            // Web-style contextmenu: fire at the same instant as
            // the OS would start showing its own context menu.
            let ctx = slot.onContextMenu
            ctx(p.x, y, 2)
        }
    }
    pub override rightMouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = BUTTONS.get(this.handle) {
            let p = this.convertPointFromView(e.locationInWindow, nil)
            let y = this.frame.size.height - p.y
            let cb = slot.onMouseUp
            cb(p.x, y, 2)
        }
    }
    /// Middle / extra mouse buttons. AppKit lumps everything that
    /// isn't left or right into the `otherMouseDown:` /
    /// `otherMouseUp:` pair. Surface as `button = 1` to match
    /// Win32's `WM_MBUTTONDOWN` mapping and Linux's
    /// `gdkBtnToW3c(2) = 1`. Side-button users on multi-button mice
    /// also land here — `NSEvent.buttonNumber` distinguishes them if
    /// callers care, but the public Signal only carries the W3C id.
    pub override otherMouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = BUTTONS.get(this.handle) {
            let p = this.convertPointFromView(e.locationInWindow, nil)
            let y = this.frame.size.height - p.y
            let cb = slot.onMouseDown
            cb(p.x, y, 1)
        }
    }
    pub override otherMouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = BUTTONS.get(this.handle) {
            let p = this.convertPointFromView(e.locationInWindow, nil)
            let y = this.frame.size.height - p.y
            let cb = slot.onMouseUp
            cb(p.x, y, 1)
        }
    }
    /// `keyDown:` / `keyUp:` are inherited from NSResponder. The
    /// bindings don't declare them on NSView so we don't need the
    /// `override` modifier here — auto-lift infers the selector
    /// (`keyDown:` / `keyUp:`) from the name + arity and emits an
    /// IMP that `register()` installs via class_addMethod.
    pub keyDown(event: NSEvent) {
        let flags = event.modifierFlags as u64
        let code = keyCodeToString(event.keyCode)
        let ctrl  = modCtrl(flags)
        let shift = modShift(flags)
        let alt   = modAlt(flags)
        let meta  = modMeta(flags)
        if let some(slot) = BUTTONS.get(this.handle) {
            let cb = slot.onKeyDown
            cb(code, ctrl, shift, alt, meta)
        }
        // Restore NSButton's default Space-press → click activation
        // that we'd otherwise lose by replacing `keyDown:` without
        // chaining to `super` (the ilang ObjC-super trampoline
        // SIGSEGVs — see crates/ilang-parser/src/item/extern_objc/
        // super_call.rs). `Enter` on a default button is handled
        // separately through `setKeyEquivalent("\r")` and doesn't
        // route through `keyDown:`, so no extra branch needed.
        if code == KeyCode.space as string {
            this.performClick(nil)
        }
    }
    pub keyUp(event: NSEvent) {
        let flags = event.modifierFlags as u64
        if let some(slot) = BUTTONS.get(this.handle) {
            let cb = slot.onKeyUp
            cb(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
}

/// Per-button "primary button is currently pressed" flag, keyed
/// by the NSButton's `handle`. `mouseDown:` sets it; `mouseUp:`
/// clears it and decides whether to synthesise `onClick` (only
/// fires when the release lands inside the bounds, mirroring DOM
/// `click`).
let PENDING_CLICK = new Map<i64, bool>()


pub fn createNativeButton(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool, isDefault: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    // Construct an ordinary `NSButton` through `initWithFrame:` so
    // the cell is set up. Then register `FocusButton` with the ObjC
    // runtime and rewrite the instance's isa pointer to it so AppKit
    // dispatches `becomeFirstResponder` / `resignFirstResponder`
    // through our IMPs.
    let btn = NSButton.alloc().init(makeRect(x, y, width, height))
    btn.setTitle(nsstr(text))
    btn.setEnabled(enabled)
    btn.setHidden(!visible)
    FocusButton.register()
    // ilang auto-lift registers the subclass with the ObjC runtime
    // under its fully-qualified module path (`gui_impl.FocusButton`
    // here), not the bare class name. Hard-coding the path keeps the
    // isa swap a single lookup; if ilang's mangling convention
    // changes the lookup returns 0 and the swap is silently skipped,
    // leaving the button as a plain `NSButton` (focus / mouse
    // events stop firing but every other behaviour stays intact).
    let focusCls = classPtr("gui_impl.FocusButton")
    if focusCls != 0 as i64 {
        let _ = setObjectClass(btn.handle, focusCls)
    }
    // Tracking area covers the whole button. `activeAlways` so
    // entered/exited fire regardless of which window is key, and
    // `inVisibleRect` ties the area to the view's bounds so a later
    // setFrame doesn't desynchronise the hit region.
    let opts = NSTrackingAreaOptions.mouseEnteredAndExited
             | NSTrackingAreaOptions.activeAlways
             | NSTrackingAreaOptions.inVisibleRect
    let area = NSTrackingArea.alloc().init(
        makeRect(0.0, 0.0, width, height), opts,
        btn as NSObject, nil
    )
    (btn as NSView).addTrackingArea(area as NSObject)

    ClickHandler.register()
    let h = ClickHandler.alloc().init()
    btn.setTarget(h)
    btn.setAction(selectorPtr("clicked:"))

    winSlot.win.contentView.addSubview(btn as NSView)

    let handle = btn.handle
    BUTTONS[handle] = new ButtonSlot(btn, h as NSObject, windowHandle)
    if isDefault { setButtonDefault(handle, true) }
    handle
}

pub fn setOnButtonClick(handle: i64, cb: fn(f64, f64, i32)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onClick = cb }
}
pub fn setButtonText(handle: i64, text: string) {
    if let some(slot) = BUTTONS.get(handle) { slot.btn.setTitle(nsstr(text)) }
}
pub fn setButtonFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = BUTTONS.get(handle) {
        slot.btn.setFrame(makeRect(x, y, width, height))
    }
}
pub fn setButtonEnabled(handle: i64, enabled: bool) {
    if let some(slot) = BUTTONS.get(handle) { slot.btn.setEnabled(enabled) }
}
pub fn setButtonVisible(handle: i64, visible: bool) {
    if let some(slot) = BUTTONS.get(handle) { slot.btn.setHidden(!visible) }
}
pub fn clickButton(handle: i64) {
    if let some(slot) = BUTTONS.get(handle) {
        // `performClick:` runs the configured target/action, which
        // is our `ClickHandler.clicked:` → forwards to `onClick`.
        slot.btn.performClick(nil)
    }
}
pub fn setOnButtonFocus(handle: i64, cb: fn()) {
    if let some(slot) = BUTTONS.get(handle) { slot.onFocus = cb }
}
pub fn setOnButtonBlur(handle: i64, cb: fn()) {
    if let some(slot) = BUTTONS.get(handle) { slot.onBlur = cb }
}
pub fn setOnButtonMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnButtonMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnButtonMouseDown(handle: i64, cb: fn(f64, f64, i32)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnButtonMouseUp(handle: i64, cb: fn(f64, f64, i32)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnButtonContextMenu(handle: i64, cb: fn(f64, f64, i32)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnButtonKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnButtonKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onKeyUp = cb }
}
pub fn setButtonTooltip(handle: i64, text: string) {
    if let some(slot) = BUTTONS.get(handle) {
        // Empty string → pass `nil` so AppKit clears the tooltip
        // outright. An empty `NSString` would register as a real
        // tooltip and pop up a stub bubble.
        if text == "" {
            (slot.btn as NSView).setToolTip(nil)
        } else {
            (slot.btn as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}
pub fn focusButton(handle: i64) {
    if let some(slot) = BUTTONS.get(handle) {
        // Ask the parent window to install this button as its
        // first responder. AppKit then drives our subclass'
        // `becomeFirstResponder` override, which fires `onFocus`.
        let win = slot.btn.window
        let _ = win.makeFirstResponder(slot.btn as NSObject)
    }
}
pub fn setButtonDefault(handle: i64, isDefault: bool) {
    if let some(slot) = BUTTONS.get(handle) {
        if isDefault {
            // Clear the previous holder in the same window so two
            // buttons never end up both responding to Return.
            if let some(prev) = DEFAULT_BUTTONS.get(slot.winHandle) {
                if prev != handle {
                    if let some(prevSlot) = BUTTONS.get(prev) {
                        prevSlot.btn.setKeyEquivalent(nsstr(""))
                    }
                }
            }
            slot.btn.setKeyEquivalent(nsstr("\r"))
            DEFAULT_BUTTONS[slot.winHandle] = handle
        } else {
            slot.btn.setKeyEquivalent(nsstr(""))
            if let some(cur) = DEFAULT_BUTTONS.get(slot.winHandle) {
                if cur == handle { DEFAULT_BUTTONS.delete(slot.winHandle) }
            }
        }
    }
}
