/// Static label. NSTextField with editable / bordered / bezeled
/// / drawsBackground all off — `makeLabel` from the cocoa
/// foundation helpers does that for us.

use cocoa {
    NSTextField, NSObject, NSView, NSEvent,
    NSTrackingArea, NSTrackingAreaOptions,
    nsstr, nil, makeRect, makeLabel,
    classPtr, setObjectClass,
}
use core {
    WINDOWS, noOpEnterLeave, noOpClickMod, noOpWheel,
    modCtrl, modShift, modAlt, modMeta,
}

class LabelSlot {
    pub label: NSTextField
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub init(label: NSTextField) {
        this.label = label
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseDown  = noOpClickMod
        this.onMouseUp    = noOpClickMod
        this.onContextMenu = noOpClickMod
        this.onMouseWheel = noOpWheel
    }
}

let LABELS = new Map<i64, LabelSlot>()

/// NSTextField subclass installed via isa-swap so AppKit's mouse
/// dispatch lands in our IMPs. Same pattern as FocusTextField /
/// FocusButton. Forwarding to super preserves whatever default
/// AppKit behaviour the parent NSTextField has — drag-select
/// when `selectable=true`, no-op otherwise.
class FocusLabel : NSTextField {
    pub override mouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
        let y = (this as NSView).frame.size.height - p.y
        let flags = e.modifierFlags as u64
        if let some(slot) = LABELS.get(this.handle) {
            let cb = slot.onMouseDown
            cb(p.x, y, 0,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        // Forward so a `selectable=true` label's drag-select still
        // works. Unlike the editable-TextField path, the default
        // label cell's trackMouse loop does not always swallow the
        // mouseUp — synthesising the up emission here would
        // double-fire onMouseUp on non-selectable labels (the
        // common case). The real `mouseUp:` override below
        // handles the up for both modes.
        super.mouseDown(event)
    }
    pub override mouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = LABELS.get(this.handle) {
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
        if let some(slot) = LABELS.get(this.handle) {
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
        if let some(slot) = LABELS.get(this.handle) {
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
        if let some(slot) = LABELS.get(this.handle) {
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
        if let some(slot) = LABELS.get(this.handle) {
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
        if let some(slot) = LABELS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let cb = slot.onMouseEnter
            cb(p.x, y)
        }
    }
    pub override mouseExited(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = LABELS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let cb = slot.onMouseLeave
            cb(p.x, y)
        }
    }
    pub scrollWheel(event: NSEvent) {
        if let some(slot) = LABELS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(event.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = event.modifierFlags as u64
            let cb = slot.onMouseWheel
            cb(event.deltaX, event.deltaY, p.x, y,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
}

pub fn createNativeLabel(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool, selectable: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let lbl = makeLabel(text, x, y, width, height, selectable)
    lbl.setHidden(!visible)
    lbl.setEnabled(enabled)
    // Swap to FocusLabel so AppKit dispatches mouse selectors
    // through our IMP. Same fragile fully-qualified class lookup
    // the FocusButton / FocusTextField paths use.
    FocusLabel.register()
    let focusCls = classPtr("gui_impl.FocusLabel")
    if focusCls != 0 as i64 {
        let _ = setObjectClass((lbl as NSView).handle, focusCls)
    }
    // Tracking area for mouseEntered: / mouseExited:.
    // `inVisibleRect` pins the area to the view's bounds so a
    // later setFrame doesn't desync the hit region.
    let opts = NSTrackingAreaOptions.mouseEnteredAndExited
             | NSTrackingAreaOptions.activeAlways
             | NSTrackingAreaOptions.inVisibleRect
    let area = NSTrackingArea.alloc().init(
        makeRect(0.0, 0.0, width, height), opts,
        lbl as NSObject, nil
    )
    (lbl as NSView).addTrackingArea(area as NSObject)
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
pub fn setLabelEnabled(handle: i64, enabled: bool) {
    if let some(slot) = LABELS.get(handle) { slot.label.setEnabled(enabled) }
}
pub fn setLabelSelectable(handle: i64, selectable: bool) {
    if let some(slot) = LABELS.get(handle) { slot.label.setSelectable(selectable) }
}
pub fn setLabelTooltip(handle: i64, text: string) {
    if let some(slot) = LABELS.get(handle) {
        // Empty string → pass `nil` so AppKit clears the tooltip
        // outright. An empty `NSString` would register as a real
        // tooltip and pop up a stub bubble.
        if text == "" {
            (slot.label as NSView).setToolTip(nil)
        } else {
            (slot.label as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}

pub fn setOnLabelMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = LABELS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnLabelMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = LABELS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnLabelMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = LABELS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnLabelMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = LABELS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnLabelContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = LABELS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnLabelMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = LABELS.get(handle) { slot.onMouseWheel = cb }
}
