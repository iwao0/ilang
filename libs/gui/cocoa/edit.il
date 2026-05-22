/// Single-line text input. NSTextField in editable mode; the
/// delegate hop on `controlTextDidChange:` forwards the new
/// contents to the registered callback.

use cocoa {
    NSObject, NSTextField, NSView, NSNotification,
    nsstr, ilstr, makeRect, makeTextField,
}
use core { WINDOWS, noOpText }

class EditSlot {
    pub edit:     NSTextField
    pub onChange: fn(string)
    pub handler:  NSObject
    pub init(edit: NSTextField, handler: NSObject) {
        this.edit = edit
        this.onChange = noOpText
        this.handler = handler
    }
}

let EDITS = new Map<i64, EditSlot>()

class ChangeHandler : NSObject {
    pub controlTextDidChange(n: NSNotification) {
        if let some(slot) = EDITS.get(n.notificationObject.handle) {
            let cb = slot.onChange
            cb(ilstr(slot.edit.stringValue))
        }
    }
}

pub fn createNativeEdit(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let tf = makeTextField(text, x, y, width, height)
    tf.setEnabled(enabled)
    tf.setHidden(!visible)

    ChangeHandler.register()
    let h = ChangeHandler.alloc().init()
    tf.setDelegate(h as NSObject)

    winSlot.win.contentView.addSubview(tf as NSView)

    let handle = (tf as NSView).handle
    EDITS[handle] = new EditSlot(tf, h as NSObject)
    handle
}

pub fn readEditText(handle: i64): string {
    match EDITS.get(handle) {
        some(slot) { ilstr(slot.edit.stringValue) }
        none       { "" }
    }
}

pub fn setOnEditChange(handle: i64, cb: fn(string)) {
    if let some(slot) = EDITS.get(handle) { slot.onChange = cb }
}
pub fn setEditText(handle: i64, text: string) {
    if let some(slot) = EDITS.get(handle) { slot.edit.stringValue = nsstr(text) }
}
pub fn setEditFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = EDITS.get(handle) {
        slot.edit.setFrame(makeRect(x, y, width, height))
    }
}
pub fn setEditEnabled(handle: i64, enabled: bool) {
    if let some(slot) = EDITS.get(handle) { slot.edit.setEnabled(enabled) }
}
pub fn setEditVisible(handle: i64, visible: bool) {
    if let some(slot) = EDITS.get(handle) { slot.edit.setHidden(!visible) }
}
