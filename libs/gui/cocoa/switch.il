/// Switch. `NSSwitch` (macOS 10.15+). Click target/action goes
/// through `SwitchToggleHandler` which reads the new `state`
/// (0 = off, 1 = on) and posts a bool.

use cocoa {
    NSObject, NSSwitch, NSView, NSEvent,
    NSTrackingArea, NSTrackingAreaOptions,
    nsstr, nil, makeRect, selectorPtr,
    classPtr, setObjectClass,
}
use core {
    WINDOWS, resolveParentView, noOp1, noOpBool, noOpKey,
    noOpEnterLeave, noOpClickMod, noOpWheel,
    keyCodeToString, modCtrl, modShift, modAlt, modMeta,
}

class SwitchSlot {
    pub sw:       NSSwitch
    pub onToggle: fn(bool)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub onKeyDown: fn(string, bool, bool, bool, bool)
    pub onKeyUp:   fn(string, bool, bool, bool, bool)
    pub handler:  NSObject
    pub init(sw: NSSwitch, handler: NSObject) {
        this.sw = sw
        this.onToggle = noOpBool
        this.onFocus  = noOp1
        this.onBlur   = noOp1
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

let SWITCHES = new Map<i64, SwitchSlot>()

/// NSSwitch subclass — isa-swapped so AppKit's responder /
/// mouse / key dispatch reaches our overrides. Mirrors the
/// `FocusCheckbox` pattern.
class FocusSwitch : NSSwitch {
    pub acceptsFirstResponder(): bool { true }
    pub becomeFirstResponder(): bool {
        if let some(slot) = SWITCHES.get(this.handle) {
            slot.onFocus()
        }
        true
    }
    pub resignFirstResponder(): bool {
        if let some(slot) = SWITCHES.get(this.handle) {
            slot.onBlur()
        }
        true
    }
    pub override mouseDown(event: NSObject) {
        let win = (this as NSView).window
        if win.firstResponder.handle != this.handle {
            let _ = win.makeFirstResponder(this as NSObject)
        }
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
        let y = (this as NSView).frame.size.height - p.y
        let flags = e.modifierFlags as u64
        if let some(slot) = SWITCHES.get(this.handle) {
            slot.onMouseDown(p.x, y, 0,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.mouseDown(event)
    }
    pub override mouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = SWITCHES.get(this.handle) {
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
        if let some(slot) = SWITCHES.get(this.handle) {
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
        if let some(slot) = SWITCHES.get(this.handle) {
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
        if let some(slot) = SWITCHES.get(this.handle) {
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
        if let some(slot) = SWITCHES.get(this.handle) {
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
        if let some(slot) = SWITCHES.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            slot.onMouseEnter(p.x, y)
        }
    }
    pub override mouseExited(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = SWITCHES.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            slot.onMouseLeave(p.x, y)
        }
    }
    pub scrollWheel(event: NSEvent) {
        if let some(slot) = SWITCHES.get(this.handle) {
            let p = (this as NSView).convertPointFromView(event.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = event.modifierFlags as u64
            slot.onMouseWheel(event.deltaX, event.deltaY, p.x, y,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
    pub keyDown(event: NSEvent) {
        if let some(slot) = SWITCHES.get(this.handle) {
            let flags = event.modifierFlags as u64
            slot.onKeyDown(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
    pub keyUp(event: NSEvent) {
        if let some(slot) = SWITCHES.get(this.handle) {
            let flags = event.modifierFlags as u64
            slot.onKeyUp(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
}

class SwitchToggleHandler : NSObject {
    pub toggled(sender: NSObject) {
        if let some(slot) = SWITCHES.get(sender.handle) {
            slot.onToggle(slot.sw.state == 1 as i64)
        }
    }
}

pub fn createNativeSwitch(
    windowHandle: i64,
    checked: bool,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let sw = NSSwitch.alloc().init(makeRect(x, y, width, height))
    sw.state = if checked { 1 as i64 } else { 0 as i64 }
    sw.setEnabled(enabled)
    (sw as NSView).setHidden(!visible)

    SwitchToggleHandler.register()
    let h = SwitchToggleHandler.alloc().init()
    sw.setTarget(h)
    sw.setAction(selectorPtr("toggled:"))

    FocusSwitch.register()
    let focusCls = classPtr("gui_impl.FocusSwitch")
    if focusCls != 0 as i64 {
        let _ = setObjectClass((sw as NSView).handle, focusCls)
    }
    let opts = NSTrackingAreaOptions.mouseEnteredAndExited
             | NSTrackingAreaOptions.activeAlways
             | NSTrackingAreaOptions.inVisibleRect
    let area = NSTrackingArea.alloc().init(
        makeRect(0.0, 0.0, width, height), opts,
        sw as NSObject, nil
    )
    (sw as NSView).addTrackingArea(area as NSObject)

    parent.addSubview(sw as NSView)

    let handle = sw.handle
    SWITCHES[handle] = new SwitchSlot(sw, h as NSObject)
    handle
}

pub fn setOnSwitchToggle(handle: i64, cb: fn(bool)) {
    if let some(slot) = SWITCHES.get(handle) { slot.onToggle = cb }
}
pub fn setSwitchChecked(handle: i64, checked: bool) {
    if let some(slot) = SWITCHES.get(handle) {
        slot.sw.state = if checked { 1 as i64 } else { 0 as i64 }
    }
}
pub fn readSwitchChecked(handle: i64): bool {
    match SWITCHES.get(handle) {
        some(slot) { slot.sw.state == 1 as i64 }
        none       { false }
    }
}
pub fn setSwitchFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = SWITCHES.get(handle) {
        (slot.sw as NSView).setFrame(makeRect(x, y, width, height))
    }
}
pub fn setSwitchEnabled(handle: i64, enabled: bool) {
    if let some(slot) = SWITCHES.get(handle) { slot.sw.setEnabled(enabled) }
}
pub fn setSwitchVisible(handle: i64, visible: bool) {
    if let some(slot) = SWITCHES.get(handle) { (slot.sw as NSView).setHidden(!visible) }
}
pub fn setOnSwitchFocus(handle: i64, cb: fn()) {
    if let some(slot) = SWITCHES.get(handle) { slot.onFocus = cb }
}
pub fn setOnSwitchBlur(handle: i64, cb: fn()) {
    if let some(slot) = SWITCHES.get(handle) { slot.onBlur = cb }
}
pub fn focusSwitch(handle: i64) {
    if let some(slot) = SWITCHES.get(handle) {
        let win = (slot.sw as NSView).window
        let _ = win.makeFirstResponder(slot.sw as NSObject)
    }
}
pub fn blurSwitch(handle: i64) {
    if let some(slot) = SWITCHES.get(handle) {
        let win = (slot.sw as NSView).window
        if win.firstResponder.handle == slot.sw.handle {
            let _ = win.makeFirstResponder(nil)
        }
    }
}
pub fn setOnSwitchMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = SWITCHES.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnSwitchMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = SWITCHES.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnSwitchMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = SWITCHES.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnSwitchMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = SWITCHES.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnSwitchContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = SWITCHES.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnSwitchMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = SWITCHES.get(handle) { slot.onMouseWheel = cb }
}
pub fn setOnSwitchKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = SWITCHES.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnSwitchKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = SWITCHES.get(handle) { slot.onKeyUp = cb }
}
pub fn setSwitchTooltip(handle: i64, text: string) {
    if let some(slot) = SWITCHES.get(handle) {
        if text == "" {
            (slot.sw as NSView).setToolTip(nil)
        } else {
            (slot.sw as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}
