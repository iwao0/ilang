/// Checkbox. NSButton with `setButtonType(NSButtonTypeSwitch=3)`.
/// Click target/action goes through `ToggleHandler` which reads
/// the new `state` (0 = off, 1 = on) and posts a bool.

use cocoa { NSObject, NSButton, NSView, nsstr, nil, makeRect, selectorPtr }
use core { WINDOWS, noOpBool }

class CheckboxSlot {
    pub btn:      NSButton
    pub onToggle: fn(bool)
    pub handler:  NSObject
    pub init(btn: NSButton, handler: NSObject) {
        this.btn = btn
        this.onToggle = noOpBool
        this.handler = handler
    }
}

let CHECKBOXES = new Map<i64, CheckboxSlot>()

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
pub fn setCheckboxTooltip(handle: i64, text: string) {
    if let some(slot) = CHECKBOXES.get(handle) {
        if text == "" {
            (slot.btn as NSView).setToolTip(nil)
        } else {
            (slot.btn as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}
