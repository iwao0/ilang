/// Continuous slider. NSSlider with `setContinuous(true)` so the
/// callback fires every time the thumb moves, not just on mouse-up.

use cocoa { NSObject, NSSlider, NSView, nsstr, nil, makeRect, makeSlider, selectorPtr }
use core { WINDOWS, noOpF64 }

class SliderSlot {
    pub slider:   NSSlider
    pub onChange: fn(f64)
    pub handler:  NSObject
    pub init(slider: NSSlider, handler: NSObject) {
        this.slider = slider
        this.onChange = noOpF64
        this.handler = handler
    }
}

let SLIDERS = new Map<i64, SliderSlot>()

class SliderHandler : NSObject {
    pub slid(sender: NSObject) {
        if let some(slot) = SLIDERS.get(sender.handle) {
            let cb = slot.onChange
            cb(slot.slider.doubleValue)
        }
    }
}

pub fn createNativeSlider(
    windowHandle: i64,
    min: f64, max: f64, value: f64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
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

    winSlot.win.contentView.addSubview(s as NSView)

    let handle = (s as NSView).handle
    SLIDERS[handle] = new SliderSlot(s, h as NSObject)
    handle
}

pub fn setOnSliderChange(handle: i64, cb: fn(f64)) {
    if let some(slot) = SLIDERS.get(handle) { slot.onChange = cb }
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
pub fn setSliderTooltip(handle: i64, text: string) {
    if let some(slot) = SLIDERS.get(handle) {
        if text == "" {
            (slot.slider as NSView).setToolTip(nil)
        } else {
            (slot.slider as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}
