/// Drop-down menu button. NSPopUpButton (`pullsDown=false`) —
/// looks like a combo box / picker with the current selection as
/// the title.

use cocoa { NSObject, NSPopUpButton, NSView, nsstr, makeRect, selectorPtr }
use core { WINDOWS, noOpI32 }

class ComboSlot {
    pub combo:   NSPopUpButton
    pub onPick:  fn(i32)
    pub handler: NSObject
    pub init(combo: NSPopUpButton, handler: NSObject) {
        this.combo = combo
        this.onPick = noOpI32
        this.handler = handler
    }
}

let COMBOS = new Map<i64, ComboSlot>()

class ComboHandler : NSObject {
    pub picked(sender: NSObject) {
        if let some(slot) = COMBOS.get(sender.handle) {
            let cb = slot.onPick
            cb(slot.combo.indexOfSelectedItem as i32)
        }
    }
}

pub fn createNativeCombo(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let p = NSPopUpButton.alloc().init(makeRect(x, y, width, height), false)
    p.setEnabled(enabled)
    p.setHidden(!visible)

    ComboHandler.register()
    let h = ComboHandler.alloc().init()
    p.setTarget(h)
    p.setAction(selectorPtr("picked:"))

    winSlot.win.contentView.addSubview(p as NSView)

    let handle = (p as NSView).handle
    COMBOS[handle] = new ComboSlot(p, h as NSObject)
    handle
}

pub fn setOnComboSelect(handle: i64, cb: fn(i32)) {
    if let some(slot) = COMBOS.get(handle) { slot.onPick = cb }
}
pub fn addComboItem(handle: i64, item: string) {
    if let some(slot) = COMBOS.get(handle) { slot.combo.addItemWithTitle(nsstr(item)) }
}
pub fn clearComboItems(handle: i64) {
    if let some(slot) = COMBOS.get(handle) { slot.combo.removeAllItems() }
}
pub fn setComboSelectedIndex(handle: i64, index: i32) {
    if let some(slot) = COMBOS.get(handle) {
        slot.combo.selectItemAtIndex(index as i64)
    }
}
pub fn readComboSelectedIndex(handle: i64): i32 {
    match COMBOS.get(handle) {
        some(slot) { slot.combo.indexOfSelectedItem as i32 }
        none       { -1 as i32 }
    }
}
pub fn setComboFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = COMBOS.get(handle) {
        slot.combo.setFrame(makeRect(x, y, width, height))
    }
}
pub fn setComboEnabled(handle: i64, enabled: bool) {
    if let some(slot) = COMBOS.get(handle) { slot.combo.setEnabled(enabled) }
}
pub fn setComboVisible(handle: i64, visible: bool) {
    if let some(slot) = COMBOS.get(handle) { slot.combo.setHidden(!visible) }
}
