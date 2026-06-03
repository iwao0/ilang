/// Push button. NSButton subview of the parent window's
/// contentView. Click target/action goes through one shared
/// `ClickHandler` that looks the sender up in `BUTTONS` by its
/// raw ObjC pointer (`obj.handle`) — same field that gui-core
/// stores as the public `handle: i64`.

use cocoa {
    NSObject, NSButton, NSView,
    nsstr, makeRect, selectorPtr, nil,
}
use core { WINDOWS, noOpClick }

/// Per-button state. Stored in `BUTTONS` keyed by the NSButton's
/// raw ObjC pointer (`btn.handle`). Same key gui-core sees as the
/// public handle, so both the public setters and the
/// click-handler-side sender lookup are O(1) Map gets.
class ButtonSlot {
    pub btn:     NSButton
    pub onClick: fn(f64, f64, i32)
    pub handler: NSObject
    pub winHandle: i64
    pub init(btn: NSButton, handler: NSObject, winHandle: i64) {
        this.btn = btn
        this.onClick = noOpClick
        this.handler = handler
        this.winHandle = winHandle
    }
}

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

pub fn createNativeButton(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool, isDefault: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let btn = NSButton.alloc().init(makeRect(x, y, width, height))
    btn.setTitle(nsstr(text))
    btn.setEnabled(enabled)
    btn.setHidden(!visible)

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
