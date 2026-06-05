/// Drop-down menu button. NSPopUpButton (`pullsDown=false`) —
/// looks like a combo box / picker with the current selection as
/// the title.

use cocoa {
    NSObject, NSPopUpButton, NSView, NSEvent,
    NSTrackingArea, NSTrackingAreaOptions,
    nsstr, nil, makeRect, selectorPtr,
    classPtr, setObjectClass,
}
use core {
    WINDOWS, resolveParentView, noOp1, noOpI32, noOpKey,
    noOpEnterLeave, noOpClickMod, noOpWheel,
    keyCodeToString, modCtrl, modShift, modAlt, modMeta,
}

class ComboSlot {
    pub combo:   NSPopUpButton
    pub onPick:  fn(i32)
    pub onFocus: fn()
    pub onBlur:  fn()
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub onKeyDown: fn(string, bool, bool, bool, bool)
    pub onKeyUp:   fn(string, bool, bool, bool, bool)
    pub handler: NSObject
    pub init(combo: NSPopUpButton, handler: NSObject) {
        this.combo = combo
        this.onPick = noOpI32
        this.onFocus = noOp1
        this.onBlur  = noOp1
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseDown  = noOpClickMod
        this.onMouseUp    = noOpClickMod
        this.onContextMenu = noOpClickMod
        this.onMouseWheel = noOpWheel
        this.onKeyDown = noOpKey
        this.onKeyUp   = noOpKey
        this.handler = handler
    }
}

let COMBOS = new Map<i64, ComboSlot>()

class FocusCombo : NSPopUpButton {
    pub acceptsFirstResponder(): bool { true }
    pub becomeFirstResponder(): bool {
        if let some(slot) = COMBOS.get(this.handle) {
            slot.onFocus()
        }
        true
    }
    pub resignFirstResponder(): bool {
        if let some(slot) = COMBOS.get(this.handle) {
            slot.onBlur()
        }
        true
    }
    pub override mouseDown(event: NSObject) {
        let win = (this as NSView).window
        if win.firstResponder.handle != (this as NSView).handle {
            let _ = win.makeFirstResponder(this as NSObject)
        }
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
        let y = (this as NSView).frame.size.height - p.y
        let flags = e.modifierFlags as u64
        if let some(slot) = COMBOS.get((this as NSView).handle) {
            slot.onMouseDown(p.x, y, 0,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.mouseDown(event)
    }
    pub override mouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = COMBOS.get((this as NSView).handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            slot.onMouseUp(p.x, y, 0,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.mouseUp(event)
    }
    pub override rightMouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = COMBOS.get((this as NSView).handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            let ck = modCtrl(flags)
            let sk = modShift(flags)
            let ak = modAlt(flags)
            let mk = modMeta(flags)
            let down = slot.onMouseDown
            down(p.x, y, 2, ck, sk, ak, mk)
            let ctx = slot.onContextMenu
            ctx(p.x, y, 2, ck, sk, ak, mk)
        }
        super.rightMouseDown(event)
    }
    pub override rightMouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = COMBOS.get((this as NSView).handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            slot.onMouseUp(p.x, y, 2,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.rightMouseUp(event)
    }
    pub override otherMouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = COMBOS.get((this as NSView).handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            slot.onMouseDown(p.x, y, 1,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.otherMouseDown(event)
    }
    pub override otherMouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = COMBOS.get((this as NSView).handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            slot.onMouseUp(p.x, y, 1,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.otherMouseUp(event)
    }
    pub override mouseEntered(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = COMBOS.get((this as NSView).handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            slot.onMouseEnter(p.x, y)
        }
    }
    pub override mouseExited(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = COMBOS.get((this as NSView).handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            slot.onMouseLeave(p.x, y)
        }
    }
    pub scrollWheel(event: NSEvent) {
        if let some(slot) = COMBOS.get((this as NSView).handle) {
            let p = (this as NSView).convertPointFromView(event.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = event.modifierFlags as u64
            slot.onMouseWheel(event.deltaX, event.deltaY, p.x, y,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
    pub keyDown(event: NSEvent) {
        if let some(slot) = COMBOS.get((this as NSView).handle) {
            let flags = event.modifierFlags as u64
            slot.onKeyDown(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
    pub keyUp(event: NSEvent) {
        if let some(slot) = COMBOS.get((this as NSView).handle) {
            let flags = event.modifierFlags as u64
            slot.onKeyUp(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
}

class ComboHandler : NSObject {
    pub picked(sender: NSObject) {
        if let some(slot) = COMBOS.get(sender.handle) {
            slot.onPick(slot.combo.indexOfSelectedItem as i32)
        }
    }
}

pub fn createNativeCombo(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
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
    let opts = NSTrackingAreaOptions.mouseEnteredAndExited
             | NSTrackingAreaOptions.activeAlways
             | NSTrackingAreaOptions.inVisibleRect
    let area = NSTrackingArea.alloc().init(
        makeRect(0.0, 0.0, width, height), opts,
        p as NSObject, nil
    )
    (p as NSView).addTrackingArea(area as NSObject)

    parent.addSubview(p as NSView)

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
pub fn setOnComboMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = COMBOS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnComboMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = COMBOS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnComboMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = COMBOS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnComboMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = COMBOS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnComboContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = COMBOS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnComboMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = COMBOS.get(handle) { slot.onMouseWheel = cb }
}
pub fn setOnComboKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = COMBOS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnComboKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = COMBOS.get(handle) { slot.onKeyUp = cb }
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
