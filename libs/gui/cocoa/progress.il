/// Progress indicator. NSProgressIndicator in `bar` style;
/// driven via `doubleValue` (no callbacks).

use cocoa {
    NSObject, NSProgressIndicator, NSView,
    NSProgressIndicatorStyle, nsstr, nil,
    makeRect, makeProgressBar,
}
use core { WINDOWS, resolveParentView }

class ProgressSlot {
    pub bar: NSProgressIndicator
    pub init(bar: NSProgressIndicator) { this.bar = bar }
}

let PROGRESS_BARS = new Map<i64, ProgressSlot>()

pub fn createNativeProgress(
    windowHandle: i64,
    min: f64, max: f64, value: f64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let p = makeProgressBar(min, max, value, x, y, width, height)
    p.setHidden(!visible)
    parent.addSubview(p as NSView)
    let handle = (p as NSView).handle
    PROGRESS_BARS[handle] = new ProgressSlot(p)
    handle
}

pub fn setProgressRange(handle: i64, min: f64, max: f64) {
    if let some(slot) = PROGRESS_BARS.get(handle) {
        slot.bar.setMinValue(min)
        slot.bar.setMaxValue(max)
    }
}
pub fn setProgressValue(handle: i64, value: f64) {
    if let some(slot) = PROGRESS_BARS.get(handle) { slot.bar.doubleValue = value }
}
pub fn setProgressFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = PROGRESS_BARS.get(handle) {
        slot.bar.setFrame(makeRect(x, y, width, height))
    }
}
pub fn setProgressVisible(handle: i64, visible: bool) {
    if let some(slot) = PROGRESS_BARS.get(handle) { slot.bar.setHidden(!visible) }
}
pub fn setProgressEnabled(handle: i64, enabled: bool) {
    // NSProgressIndicator has no `enabled` state — progress bars
    // are non-interactive on macOS, surface as a no-op.
}
pub fn setProgressTooltip(handle: i64, text: string) {
    if let some(slot) = PROGRESS_BARS.get(handle) {
        if text == "" {
            (slot.bar as NSView).setToolTip(nil)
        } else {
            (slot.bar as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}
pub fn setProgressIndeterminate(handle: i64, on: bool) {
    if let some(slot) = PROGRESS_BARS.get(handle) {
        slot.bar.setIndeterminate(on)
        if on { slot.bar.startAnimation(nil) }
        else  { slot.bar.stopAnimation(nil) }
    }
}
pub fn setProgressOrientation(handle: i64, vertical: bool) {
    // NSProgressIndicator follows its frame's aspect ratio — a
    // taller-than-wide frame renders vertically. The public class
    // sizes the bar before calling this, so no native call is
    // needed beyond keeping the API surface consistent.
}
