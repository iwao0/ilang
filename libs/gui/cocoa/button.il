/// Push button. NSButton subview of the parent window's
/// contentView. Click target/action goes through one shared
/// `ClickHandler` that looks the sender up in `BUTTONS` by its
/// raw ObjC pointer (`obj.handle`) — same field that gui-core
/// stores as the public `handle: i64`.

use cocoa {
    NSObject, NSButton, NSView,
    nsstr, makeRect, selectorPtr,
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
    pub init(btn: NSButton, handler: NSObject) {
        this.btn = btn
        this.onClick = noOpClick
        this.handler = handler
    }
}

let BUTTONS = new Map<i64, ButtonSlot>()

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
    enabled: bool, visible: bool
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
    BUTTONS[handle] = new ButtonSlot(btn, h as NSObject)
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
