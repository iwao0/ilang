/// Radio button. NSButton with `setButtonType(NSButtonTypeRadio=4)`.
/// Mutual exclusion is driven by gui-core's per-Window
/// `_unselectSiblings` walk, so this backend just surfaces the
/// click and lets the core toggle siblings via `setRadioChecked`.

use cocoa { NSObject, NSButton, NSView, nsstr, makeRect, selectorPtr }
use core { WINDOWS, noOpBool }

class RadioSlot {
    pub btn:      NSButton
    pub onToggle: fn(bool)
    pub handler:  NSObject
    pub init(btn: NSButton, handler: NSObject) {
        this.btn = btn
        this.onToggle = noOpBool
        this.handler = handler
    }
}

let RADIOS = new Map<i64, RadioSlot>()

class RadioHandler : NSObject {
    pub radioClicked(sender: NSObject) {
        if let some(slot) = RADIOS.get(sender.handle) {
            let cb = slot.onToggle
            cb(slot.btn.state == 1 as i64)
        }
    }
}

pub fn createNativeRadio(
    windowHandle: i64,
    text: string, checked: bool,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
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

    winSlot.win.contentView.addSubview(r as NSView)

    let handle = r.handle
    RADIOS[handle] = new RadioSlot(r, h as NSObject)
    handle
}

pub fn setOnRadioToggle(handle: i64, cb: fn(bool)) {
    if let some(slot) = RADIOS.get(handle) { slot.onToggle = cb }
}
pub fn setRadioText(handle: i64, text: string) {
    if let some(slot) = RADIOS.get(handle) { slot.btn.setTitle(nsstr(text)) }
}
pub fn setRadioChecked(handle: i64, checked: bool) {
    if let some(slot) = RADIOS.get(handle) {
        slot.btn.state = if checked { 1 as i64 } else { 0 as i64 }
    }
}
pub fn readRadioChecked(handle: i64): bool {
    match RADIOS.get(handle) {
        some(slot) { slot.btn.state == 1 as i64 }
        none       { false }
    }
}
pub fn setRadioFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = RADIOS.get(handle) {
        slot.btn.setFrame(makeRect(x, y, width, height))
    }
}
pub fn setRadioEnabled(handle: i64, enabled: bool) {
    if let some(slot) = RADIOS.get(handle) { slot.btn.setEnabled(enabled) }
}
pub fn setRadioVisible(handle: i64, visible: bool) {
    if let some(slot) = RADIOS.get(handle) { slot.btn.setHidden(!visible) }
}
