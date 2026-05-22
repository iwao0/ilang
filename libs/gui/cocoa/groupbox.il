/// Titled group container. NSBox with the default
/// `boxType=primary` look (rounded border + small title).

use cocoa { NSBox, NSView, nsstr, makeRect }
use core { WINDOWS }

class GroupBoxSlot {
    pub box: NSBox
    pub init(box: NSBox) { this.box = box }
}

let GROUPBOXES = new Map<i64, GroupBoxSlot>()

pub fn createNativeGroupBox(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let box = NSBox.alloc().init(makeRect(x, y, width, height))
    box.setTitle(nsstr(text))
    box.setHidden(!visible)
    winSlot.win.contentView.addSubview(box as NSView)
    let handle = (box as NSView).handle
    GROUPBOXES[handle] = new GroupBoxSlot(box)
    handle
}

pub fn setGroupBoxText(handle: i64, text: string) {
    if let some(slot) = GROUPBOXES.get(handle) { slot.box.setTitle(nsstr(text)) }
}
pub fn setGroupBoxFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = GROUPBOXES.get(handle) {
        slot.box.setFrame(makeRect(x, y, width, height))
    }
}
pub fn setGroupBoxVisible(handle: i64, visible: bool) {
    if let some(slot) = GROUPBOXES.get(handle) { slot.box.setHidden(!visible) }
}
