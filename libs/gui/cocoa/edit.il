/// Single-line text input. NSTextField in editable mode; the
/// delegate hop on `controlTextDidChange:` forwards the new
/// contents to the registered callback.

use cocoa {
    NSObject, NSTextField, NSView, NSNotification,
    nsstr, ilstr, makeRect, makeTextField, nil,
}
use core { WINDOWS, noOpText }

class EditSlot {
    pub edit:     NSTextField
    pub onChange: fn(string)
    pub handler:  NSObject
    /// Character cap. `0` = no limit. AppKit has no built-in max
    /// length on NSTextField; the change-handler reads this and
    /// truncates `stringValue` whenever a notification arrives
    /// with a string longer than the cap.
    pub maxLength: i32
    pub init(edit: NSTextField, handler: NSObject) {
        this.edit = edit
        this.onChange = noOpText
        this.handler = handler
        this.maxLength = 0 as i32
    }
}

let EDITS = new Map<i64, EditSlot>()

class ChangeHandler : NSObject {
    pub controlTextDidChange(n: NSNotification) {
        if let some(slot) = EDITS.get(n.notificationObject.handle) {
            let s = ilstr(slot.edit.stringValue)
            // Honour `maxLength` here so a too-long string never
            // reaches the user callback. Truncate, write back, then
            // surface the trimmed value as if the user typed it.
            // The write-back fires another `controlTextDidChange:`
            // but the length is now within the cap so the second
            // pass is a no-op.
            let cap = slot.maxLength
            if cap as i64 > 0 as i64 && s.length > cap as i64 {
                let trimmed = s.slice(0 as i64, cap as i64)
                slot.edit.stringValue = nsstr(trimmed)
                let cb = slot.onChange
                cb(trimmed)
                return
            }
            let cb = slot.onChange
            cb(s)
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
pub fn setEditPlaceholder(handle: i64, text: string) {
    if let some(slot) = EDITS.get(handle) {
        slot.edit.setPlaceholderString(nsstr(text))
    }
}
pub fn setEditReadOnly(handle: i64, readOnly: bool) {
    if let some(slot) = EDITS.get(handle) { slot.edit.setEditable(!readOnly) }
}
pub fn setEditMaxLength(handle: i64, max: i32) {
    if let some(slot) = EDITS.get(handle) { slot.maxLength = max }
}
pub fn setEditTooltip(handle: i64, text: string) {
    if let some(slot) = EDITS.get(handle) {
        // Empty string → pass `nil` so AppKit clears the tooltip
        // outright rather than registering an empty bubble.
        if text == "" {
            (slot.edit as NSView).setToolTip(nil)
        } else {
            (slot.edit as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}
