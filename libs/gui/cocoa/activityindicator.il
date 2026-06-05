/// ActivityIndicator. `NSProgressIndicator` switched to
/// `spinner` style (indeterminate; `startAnimation:` /
/// `stopAnimation:` drive the spin). Slot keyed in `INDICATORS`
/// by the NSProgressIndicator handle.

use cocoa {
    NSObject, NSProgressIndicator, NSProgressIndicatorStyle, NSView,
    nsstr, nil, makeRect,
}
use core { resolveParentView }

class ActivityIndicatorSlot {
    pub ind: NSProgressIndicator
    pub init(ind: NSProgressIndicator) { this.ind = ind }
}

let INDICATORS = new Map<i64, ActivityIndicatorSlot>()

pub fn createNativeActivityIndicator(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let ind = NSProgressIndicator.alloc().init(makeRect(x, y, width, height))
    ind.setStyle(NSProgressIndicatorStyle.spinner)
    ind.setIndeterminate(true)
    (ind as NSView).setHidden(!visible)
    parent.addSubview(ind as NSView)
    let handle = ind.handle
    INDICATORS[handle] = new ActivityIndicatorSlot(ind)
    handle
}

pub fn startActivityIndicator(handle: i64) {
    if let some(slot) = INDICATORS.get(handle) { slot.ind.startAnimation(nil) }
}
pub fn stopActivityIndicator(handle: i64) {
    if let some(slot) = INDICATORS.get(handle) { slot.ind.stopAnimation(nil) }
}
pub fn setActivityIndicatorFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = INDICATORS.get(handle) {
        (slot.ind as NSView).setFrame(makeRect(x, y, width, height))
    }
}
pub fn setActivityIndicatorVisible(handle: i64, visible: bool) {
    if let some(slot) = INDICATORS.get(handle) {
        (slot.ind as NSView).setHidden(!visible)
    }
}
pub fn setActivityIndicatorTooltip(handle: i64, text: string) {
    if let some(slot) = INDICATORS.get(handle) {
        if text == "" {
            (slot.ind as NSView).setToolTip(nil)
        } else {
            (slot.ind as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}
