/// Continuous slider. NSSlider with `setContinuous(true)` so the
/// callback fires every time the thumb moves, not just on mouse-up.

use cocoa {
    NSObject, NSSlider, NSView, NSEvent,
    NSTrackingArea, NSTrackingAreaOptions,
    nsstr, nil, makeRect, makeSlider, selectorPtr,
    classPtr, setObjectClass,
}
use core {
    WINDOWS, resolveParentView, noOp1, noOpF64, noOpKey,
    noOpEnterLeave, noOpClickMod, noOpWheel,
    keyCodeToString, modCtrl, modShift, modAlt, modMeta,
}

class SliderSlot {
    pub slider:   NSSlider
    pub onChange:  fn(f64)
    pub onRelease: fn(f64)
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
    pub init(slider: NSSlider, handler: NSObject) {
        this.slider = slider
        this.onChange  = noOpF64
        this.onRelease = noOpF64
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

let SLIDERS = new Map<i64, SliderSlot>()

class FocusSlider : NSSlider {
    pub acceptsFirstResponder(): bool { true }
    pub becomeFirstResponder(): bool {
        if let some(slot) = SLIDERS.get(this.handle) {
            slot.onFocus()
        }
        true
    }
    pub resignFirstResponder(): bool {
        if let some(slot) = SLIDERS.get(this.handle) {
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
        if let some(slot) = SLIDERS.get((this as NSView).handle) {
            slot.onMouseDown(p.x, y, 0,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.mouseDown(event)
    }
    pub override mouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = SLIDERS.get((this as NSView).handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            slot.onMouseUp(p.x, y, 0,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
            // Drag-end commit. NSSlider is continuous so the
            // change callback already fired during the drag; mirror
            // the win32 / GTK `onRelease` shape by emitting once
            // more with the final value.
            let rel = slot.onRelease
            rel(slot.slider.doubleValue)
        }
        super.mouseUp(event)
    }
    pub override rightMouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = SLIDERS.get((this as NSView).handle) {
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
        if let some(slot) = SLIDERS.get((this as NSView).handle) {
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
        if let some(slot) = SLIDERS.get((this as NSView).handle) {
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
        if let some(slot) = SLIDERS.get((this as NSView).handle) {
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
        if let some(slot) = SLIDERS.get((this as NSView).handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            slot.onMouseEnter(p.x, y)
        }
    }
    pub override mouseExited(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = SLIDERS.get((this as NSView).handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            slot.onMouseLeave(p.x, y)
        }
    }
    pub scrollWheel(event: NSEvent) {
        if let some(slot) = SLIDERS.get((this as NSView).handle) {
            let p = (this as NSView).convertPointFromView(event.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = event.modifierFlags as u64
            slot.onMouseWheel(event.deltaX, event.deltaY, p.x, y,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
    pub keyDown(event: NSEvent) {
        if let some(slot) = SLIDERS.get((this as NSView).handle) {
            let flags = event.modifierFlags as u64
            slot.onKeyDown(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        // Forward so arrow-key value increments stay alive.
        super.keyDown(event)
    }
    pub keyUp(event: NSEvent) {
        if let some(slot) = SLIDERS.get((this as NSView).handle) {
            let flags = event.modifierFlags as u64
            slot.onKeyUp(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.keyUp(event)
    }
}

class SliderHandler : NSObject {
    pub slid(sender: NSObject) {
        if let some(slot) = SLIDERS.get(sender.handle) {
            slot.onChange(slot.slider.doubleValue)
        }
    }
}

pub fn createNativeSlider(
    windowHandle: i64,
    min: f64, max: f64, value: f64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let s = makeSlider(min, max, value, x, y, width, height)
    s.setContinuous(true)
    s.setHidden(!visible)
    let _ = enabled   // NSSlider has no setEnabled in our binding;
                      // surface omitted on macOS for now.

    SliderHandler.register()
    let h = SliderHandler.alloc().init()
    s.setTarget(h)
    s.setAction(selectorPtr("slid:"))

    FocusSlider.register()
    let focusCls = classPtr("gui_impl.FocusSlider")
    if focusCls != 0 as i64 {
        let _ = setObjectClass((s as NSView).handle, focusCls)
    }
    let opts = NSTrackingAreaOptions.mouseEnteredAndExited
             | NSTrackingAreaOptions.activeAlways
             | NSTrackingAreaOptions.inVisibleRect
    let area = NSTrackingArea.alloc().init(
        makeRect(0.0, 0.0, width, height), opts,
        s as NSObject, nil
    )
    (s as NSView).addTrackingArea(area as NSObject)

    parent.addSubview(s as NSView)

    let handle = (s as NSView).handle
    SLIDERS[handle] = new SliderSlot(s, h as NSObject)
    handle
}

pub fn setOnSliderChange(handle: i64, cb: fn(f64)) {
    if let some(slot) = SLIDERS.get(handle) { slot.onChange = cb }
}
pub fn setOnSliderRelease(handle: i64, cb: fn(f64)) {
    if let some(slot) = SLIDERS.get(handle) { slot.onRelease = cb }
}
pub fn setSliderTickInterval(handle: i64, min: f64, max: f64, interval: f64) {
    if let some(slot) = SLIDERS.get(handle) {
        if interval <= 0.0 {
            slot.slider.setNumberOfTickMarks(0 as i64)
        } else {
            // NSSlider exposes tick count rather than spacing. Map
            // `interval` to `count = floor((max-min)/interval) + 1`
            // so the endpoints line up the way win32 / GTK ticks do.
            let span = max - min
            let n = (span / interval) as i64 + 1 as i64
            slot.slider.setNumberOfTickMarks(n)
        }
    }
}
pub fn setSliderPageStep(handle: i64, step: f64) {
    // NSSlider has no PageUp / PageDown shortcut, so this is a
    // no-op on macOS. Arrow-key increments are derived from the
    // slider's value range automatically.
}
pub fn setSliderRange(handle: i64, min: f64, max: f64) {
    if let some(slot) = SLIDERS.get(handle) {
        slot.slider.setMinValue(min)
        slot.slider.setMaxValue(max)
    }
}
pub fn setSliderValue(handle: i64, value: f64) {
    if let some(slot) = SLIDERS.get(handle) { slot.slider.doubleValue = value }
}
pub fn readSliderValue(handle: i64): f64 {
    match SLIDERS.get(handle) {
        some(slot) { slot.slider.doubleValue }
        none       { 0.0 }
    }
}
pub fn setSliderFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = SLIDERS.get(handle) {
        slot.slider.setFrame(makeRect(x, y, width, height))
    }
}
pub fn setSliderEnabled(handle: i64, enabled: bool) {
    // NSSlider inherits setEnabled from NSControl; the binding
    // doesn't expose it today, so this is a no-op on macOS.
}
pub fn setSliderVisible(handle: i64, visible: bool) {
    if let some(slot) = SLIDERS.get(handle) { slot.slider.setHidden(!visible) }
}
pub fn setOnSliderFocus(handle: i64, cb: fn()) {
    if let some(slot) = SLIDERS.get(handle) { slot.onFocus = cb }
}
pub fn setOnSliderBlur(handle: i64, cb: fn()) {
    if let some(slot) = SLIDERS.get(handle) { slot.onBlur = cb }
}
pub fn focusSlider(handle: i64) {
    if let some(slot) = SLIDERS.get(handle) {
        let win = (slot.slider as NSView).window
        let _ = win.makeFirstResponder(slot.slider as NSObject)
    }
}
pub fn blurSlider(handle: i64) {
    if let some(slot) = SLIDERS.get(handle) {
        let win = (slot.slider as NSView).window
        if win.firstResponder.handle == (slot.slider as NSView).handle {
            let _ = win.makeFirstResponder(nil)
        }
    }
}
pub fn setOnSliderMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = SLIDERS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnSliderMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = SLIDERS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnSliderMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = SLIDERS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnSliderMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = SLIDERS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnSliderContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = SLIDERS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnSliderMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = SLIDERS.get(handle) { slot.onMouseWheel = cb }
}
pub fn setOnSliderKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = SLIDERS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnSliderKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = SLIDERS.get(handle) { slot.onKeyUp = cb }
}
pub fn setSliderTooltip(handle: i64, text: string) {
    if let some(slot) = SLIDERS.get(handle) {
        if text == "" {
            (slot.slider as NSView).setToolTip(nil)
        } else {
            (slot.slider as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}
