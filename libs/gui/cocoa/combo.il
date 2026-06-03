/// Drop-down menu button. NSPopUpButton (`pullsDown=false`) —
/// looks like a combo box / picker with the current selection as
/// the title.

use cocoa {
    NSObject, NSPopUpButton, NSView, nsstr, nil, makeRect, selectorPtr,
    classPtr, setObjectClass,
}
use core { WINDOWS, noOp1, noOpI32 }

class ComboSlot {
    pub combo:   NSPopUpButton
    pub onPick:  fn(i32)
    pub onFocus: fn()
    pub onBlur:  fn()
    pub handler: NSObject
    pub init(combo: NSPopUpButton, handler: NSObject) {
        this.combo = combo
        this.onPick = noOpI32
        this.onFocus = noOp1
        this.onBlur  = noOp1
        this.handler = handler
    }
}

let COMBOS = new Map<i64, ComboSlot>()

class FocusCombo : NSPopUpButton {
    pub acceptsFirstResponder(): bool { true }
    pub becomeFirstResponder(): bool {
        if let some(slot) = COMBOS.get(this.handle) {
            let cb = slot.onFocus
            cb()
        }
        true
    }
    pub resignFirstResponder(): bool {
        if let some(slot) = COMBOS.get(this.handle) {
            let cb = slot.onBlur
            cb()
        }
        true
    }
    pub override mouseDown(event: NSObject) {
        let win = (this as NSView).window
        if win.firstResponder.handle != (this as NSView).handle {
            let _ = win.makeFirstResponder(this as NSObject)
        }
        super.mouseDown(event)
    }
}

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

    FocusCombo.register()
    let focusCls = classPtr("gui_impl.FocusCombo")
    if focusCls != 0 as i64 {
        let _ = setObjectClass((p as NSView).handle, focusCls)
    }

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
pub fn setOnComboFocus(handle: i64, cb: fn()) {
    if let some(slot) = COMBOS.get(handle) { slot.onFocus = cb }
}
pub fn setOnComboBlur(handle: i64, cb: fn()) {
    if let some(slot) = COMBOS.get(handle) { slot.onBlur = cb }
}
pub fn focusCombo(handle: i64) {
    if let some(slot) = COMBOS.get(handle) {
        let win = (slot.combo as NSView).window
        let _ = win.makeFirstResponder(slot.combo as NSObject)
    }
}
pub fn blurCombo(handle: i64) {
    if let some(slot) = COMBOS.get(handle) {
        let win = (slot.combo as NSView).window
        if win.firstResponder.handle == (slot.combo as NSView).handle {
            let _ = win.makeFirstResponder(nil)
        }
    }
}
pub fn setComboTooltip(handle: i64, text: string) {
    if let some(slot) = COMBOS.get(handle) {
        if text == "" {
            (slot.combo as NSView).setToolTip(nil)
        } else {
            (slot.combo as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}
