/// Checkbox. NSButton with `setButtonType(NSButtonTypeSwitch=3)`.
/// Click target/action goes through `ToggleHandler` which reads
/// the new `state` (0 = off, 1 = on) and posts a bool.

use cocoa {
    NSObject, NSButton, NSView, nsstr, nil, makeRect, selectorPtr,
    classPtr, setObjectClass,
}
use core { WINDOWS, noOp1, noOpBool }

class CheckboxSlot {
    pub btn:      NSButton
    pub onToggle: fn(bool)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub handler:  NSObject
    pub init(btn: NSButton, handler: NSObject) {
        this.btn = btn
        this.onToggle = noOpBool
        this.onFocus  = noOp1
        this.onBlur   = noOp1
        this.handler = handler
    }
}

let CHECKBOXES = new Map<i64, CheckboxSlot>()

/// NSButton subclass — isa-swapped onto the freshly-created
/// checkbox so AppKit's becomeFirstResponder / resignFirstResponder
/// land in our IMPs and we can surface onFocus / onBlur. NSButton's
/// default acceptsFirstResponder is gated on Full Keyboard Access;
/// override to always accept.
class FocusCheckbox : NSButton {
    pub acceptsFirstResponder(): bool { true }
    pub becomeFirstResponder(): bool {
        if let some(slot) = CHECKBOXES.get(this.handle) {
            let cb = slot.onFocus
            cb()
        }
        true
    }
    pub resignFirstResponder(): bool {
        if let some(slot) = CHECKBOXES.get(this.handle) {
            let cb = slot.onBlur
            cb()
        }
        true
    }
    /// AppKit's NSButton mouseDown doesn't grab first responder
    /// (macOS convention — click-action controls don't shift
    /// focus). Take it explicitly so onFocus fires on click,
    /// matching the Win32 / GTK behaviour.
    pub override mouseDown(event: NSObject) {
        let win = (this as NSView).window
        if win.firstResponder.handle != this.handle {
            let _ = win.makeFirstResponder(this as NSObject)
        }
        super.mouseDown(event)
    }
}

class ToggleHandler : NSObject {
    pub toggled(sender: NSObject) {
        if let some(slot) = CHECKBOXES.get(sender.handle) {
            let cb = slot.onToggle
            // NSControlStateValueOn = 1.
            cb(slot.btn.state == 1 as i64)
        }
    }
}

pub fn createNativeCheckbox(
    windowHandle: i64,
    text: string, checked: bool,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
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

    // Swap to FocusCheckbox so AppKit's responder dispatch lands
    // in our overrides. Same isa-swap pattern as FocusButton.
    FocusCheckbox.register()
    let focusCls = classPtr("gui_impl.FocusCheckbox")
    if focusCls != 0 as i64 {
        let _ = setObjectClass((cb as NSView).handle, focusCls)
    }

    winSlot.win.contentView.addSubview(cb as NSView)

    let handle = cb.handle
    CHECKBOXES[handle] = new CheckboxSlot(cb, h as NSObject)
    handle
}

pub fn setOnCheckboxToggle(handle: i64, cb: fn(bool)) {
    if let some(slot) = CHECKBOXES.get(handle) { slot.onToggle = cb }
}
pub fn setCheckboxText(handle: i64, text: string) {
    if let some(slot) = CHECKBOXES.get(handle) { slot.btn.setTitle(nsstr(text)) }
}
pub fn setCheckboxChecked(handle: i64, checked: bool) {
    if let some(slot) = CHECKBOXES.get(handle) {
        slot.btn.state = if checked { 1 as i64 } else { 0 as i64 }
    }
}
pub fn readCheckboxChecked(handle: i64): bool {
    match CHECKBOXES.get(handle) {
        some(slot) { slot.btn.state == 1 as i64 }
        none       { false }
    }
}
pub fn setCheckboxFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = CHECKBOXES.get(handle) {
        slot.btn.setFrame(makeRect(x, y, width, height))
    }
}
pub fn setCheckboxEnabled(handle: i64, enabled: bool) {
    if let some(slot) = CHECKBOXES.get(handle) { slot.btn.setEnabled(enabled) }
}
pub fn setCheckboxVisible(handle: i64, visible: bool) {
    if let some(slot) = CHECKBOXES.get(handle) { slot.btn.setHidden(!visible) }
}
pub fn setOnCheckboxFocus(handle: i64, cb: fn()) {
    if let some(slot) = CHECKBOXES.get(handle) { slot.onFocus = cb }
}
pub fn setOnCheckboxBlur(handle: i64, cb: fn()) {
    if let some(slot) = CHECKBOXES.get(handle) { slot.onBlur = cb }
}
pub fn focusCheckbox(handle: i64) {
    if let some(slot) = CHECKBOXES.get(handle) {
        let win = slot.btn.window
        let _ = win.makeFirstResponder(slot.btn as NSObject)
    }
}
pub fn blurCheckbox(handle: i64) {
    if let some(slot) = CHECKBOXES.get(handle) {
        let win = slot.btn.window
        if win.firstResponder.handle == slot.btn.handle {
            let _ = win.makeFirstResponder(nil)
        }
    }
}
pub fn setCheckboxTooltip(handle: i64, text: string) {
    if let some(slot) = CHECKBOXES.get(handle) {
        if text == "" {
            (slot.btn as NSView).setToolTip(nil)
        } else {
            (slot.btn as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}
