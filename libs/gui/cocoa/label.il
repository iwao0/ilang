/// Static label. NSTextField with editable / bordered / bezeled
/// / drawsBackground all off — `makeLabel` from the cocoa
/// foundation helpers does that for us.

use cocoa { NSTextField, NSView, nsstr, makeRect, makeLabel }
use core { WINDOWS }

class LabelSlot {
    pub label: NSTextField
    pub init(label: NSTextField) { this.label = label }
}

let LABELS = new Map<i64, LabelSlot>()

pub fn createNativeLabel(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let lbl = makeLabel(text, x, y, width, height)
    lbl.setHidden(!visible)
    winSlot.win.contentView.addSubview(lbl as NSView)
    let handle = (lbl as NSView).handle
    LABELS[handle] = new LabelSlot(lbl)
    handle
}

pub fn setLabelText(handle: i64, text: string) {
    if let some(slot) = LABELS.get(handle) { slot.label.stringValue = nsstr(text) }
}
pub fn setLabelFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = LABELS.get(handle) {
        slot.label.setFrame(makeRect(x, y, width, height))
    }
}
pub fn setLabelVisible(handle: i64, visible: bool) {
    if let some(slot) = LABELS.get(handle) { slot.label.setHidden(!visible) }
}
