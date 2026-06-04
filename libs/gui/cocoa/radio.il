/// Radio button. NSButton with `setButtonType(NSButtonTypeRadio=4)`.
/// Mutual exclusion is driven by gui-core's per-Window
/// `_unselectSiblings` walk, so this backend just surfaces the
/// click and lets the core toggle siblings via `setRadioChecked`.

use cocoa {
    NSObject, NSButton, NSView, NSEvent,
    NSTrackingArea, NSTrackingAreaOptions,
    nsstr, nil, makeRect, selectorPtr,
    classPtr, setObjectClass,
}
use core {
    WINDOWS, resolveParentView, noOp1, noOpBool, noOpKey,
    noOpEnterLeave, noOpClickMod, noOpWheel,
    keyCodeToString, modCtrl, modShift, modAlt, modMeta,
}

class RadioSlot {
    pub btn:      NSButton
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
    pub init(btn: NSButton, handler: NSObject) {
        this.btn = btn
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

let RADIOS = new Map<i64, RadioSlot>()

class FocusRadio : NSButton {
    pub acceptsFirstResponder(): bool { true }
    pub becomeFirstResponder(): bool {
        if let some(slot) = RADIOS.get(this.handle) {
            let cb = slot.onFocus
            cb()
        }
        true
    }
    pub resignFirstResponder(): bool {
        if let some(slot) = RADIOS.get(this.handle) {
            let cb = slot.onBlur
            cb()
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
        if let some(slot) = RADIOS.get(this.handle) {
            let cb = slot.onMouseDown
            cb(p.x, y, 0,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.mouseDown(event)
    }
    pub override mouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = RADIOS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            let cb = slot.onMouseUp
            cb(p.x, y, 0,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.mouseUp(event)
    }
    pub override rightMouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = RADIOS.get(this.handle) {
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
        if let some(slot) = RADIOS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            let cb = slot.onMouseUp
            cb(p.x, y, 2,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.rightMouseUp(event)
    }
    pub override otherMouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = RADIOS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            let cb = slot.onMouseDown
            cb(p.x, y, 1,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.otherMouseDown(event)
    }
    pub override otherMouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = RADIOS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            let cb = slot.onMouseUp
            cb(p.x, y, 1,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.otherMouseUp(event)
    }
    pub override mouseEntered(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = RADIOS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let cb = slot.onMouseEnter
            cb(p.x, y)
        }
    }
    pub override mouseExited(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = RADIOS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let cb = slot.onMouseLeave
            cb(p.x, y)
        }
    }
    pub scrollWheel(event: NSEvent) {
        if let some(slot) = RADIOS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(event.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = event.modifierFlags as u64
            let cb = slot.onMouseWheel
            cb(event.deltaX, event.deltaY, p.x, y,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
    pub keyDown(event: NSEvent) {
        if let some(slot) = RADIOS.get(this.handle) {
            let flags = event.modifierFlags as u64
            let cb = slot.onKeyDown
            cb(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
    pub keyUp(event: NSEvent) {
        if let some(slot) = RADIOS.get(this.handle) {
            let flags = event.modifierFlags as u64
            let cb = slot.onKeyUp
            cb(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
}

class RadioHandler : NSObject {
    pub radioClicked(sender: NSObject) {
        if let some(slot) = RADIOS.get(sender.handle) {
            let cb = slot.onToggle
            cb(slot.btn.state == 1 as i64)
        }
    }
}

pub fn createNativeRadio(
    windowHandle: i64,
    text: string, checked: bool,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let r = NSButton.alloc().init(makeRect(x, y, width, height))
    // NSButtonTypeRadio = 4.
    r.setButtonType(4 as i64)
    r.setTitle(nsstr(text))
    r.state = if checked { 1 as i64 } else { 0 as i64 }
    r.setEnabled(enabled)
    r.setHidden(!visible)

    RadioHandler.register()
    let h = RadioHandler.alloc().init()
    r.setTarget(h)
    r.setAction(selectorPtr("radioClicked:"))

    FocusRadio.register()
    let focusCls = classPtr("gui_impl.FocusRadio")
    if focusCls != 0 as i64 {
        let _ = setObjectClass((r as NSView).handle, focusCls)
    }
    let opts = NSTrackingAreaOptions.mouseEnteredAndExited
             | NSTrackingAreaOptions.activeAlways
             | NSTrackingAreaOptions.inVisibleRect
    let area = NSTrackingArea.alloc().init(
        makeRect(0.0, 0.0, width, height), opts,
        r as NSObject, nil
    )
    (r as NSView).addTrackingArea(area as NSObject)

    parent.addSubview(r as NSView)

    let handle = r.handle
    RADIOS[handle] = new RadioSlot(r, h as NSObject)
    handle
}

pub fn setOnRadioToggle(handle: i64, cb: fn(bool)) {
    if let some(slot) = RADIOS.get(handle) { slot.onToggle = cb }
}
pub fn setRadioText(handle: i64, text: string) {
    if let some(slot) = RADIOS.get(handle) { slot.btn.setTitle(nsstr(text)) }
}
pub fn setRadioChecked(handle: i64, checked: bool) {
    if let some(slot) = RADIOS.get(handle) {
        slot.btn.state = if checked { 1 as i64 } else { 0 as i64 }
    }
}
pub fn readRadioChecked(handle: i64): bool {
    match RADIOS.get(handle) {
        some(slot) { slot.btn.state == 1 as i64 }
        none       { false }
    }
}
pub fn setRadioFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = RADIOS.get(handle) {
        slot.btn.setFrame(makeRect(x, y, width, height))
    }
}
pub fn setRadioEnabled(handle: i64, enabled: bool) {
    if let some(slot) = RADIOS.get(handle) { slot.btn.setEnabled(enabled) }
}
pub fn setRadioVisible(handle: i64, visible: bool) {
    if let some(slot) = RADIOS.get(handle) { slot.btn.setHidden(!visible) }
}
pub fn setOnRadioFocus(handle: i64, cb: fn()) {
    if let some(slot) = RADIOS.get(handle) { slot.onFocus = cb }
}
pub fn setOnRadioBlur(handle: i64, cb: fn()) {
    if let some(slot) = RADIOS.get(handle) { slot.onBlur = cb }
}
pub fn focusRadio(handle: i64) {
    if let some(slot) = RADIOS.get(handle) {
        let win = slot.btn.window
        let _ = win.makeFirstResponder(slot.btn as NSObject)
    }
}
pub fn blurRadio(handle: i64) {
    if let some(slot) = RADIOS.get(handle) {
        let win = slot.btn.window
        if win.firstResponder.handle == slot.btn.handle {
            let _ = win.makeFirstResponder(nil)
        }
    }
}
pub fn setOnRadioMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = RADIOS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnRadioMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = RADIOS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnRadioMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = RADIOS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnRadioMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = RADIOS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnRadioContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = RADIOS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnRadioMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = RADIOS.get(handle) { slot.onMouseWheel = cb }
}
pub fn setOnRadioKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = RADIOS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnRadioKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = RADIOS.get(handle) { slot.onKeyUp = cb }
}
pub fn setRadioTooltip(handle: i64, text: string) {
    if let some(slot) = RADIOS.get(handle) {
        if text == "" {
            (slot.btn as NSView).setToolTip(nil)
        } else {
            (slot.btn as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}
