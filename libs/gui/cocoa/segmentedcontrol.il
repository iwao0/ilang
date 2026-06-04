/// SegmentedControl. `NSSegmentedControl` in `selectOne` tracking
/// mode. target/action routes through `SegmentedHandler` which
/// reads `selectedSegment` and posts the new index.

use cocoa {
    NSObject, NSSegmentedControl, NSSegmentSwitchTracking,
    NSView,
    nsstr, nil, makeRect, selectorPtr,
}
use core { resolveParentView }

class SegmentedSlot {
    pub ctl:      NSSegmentedControl
    pub titles:   string[]
    pub onChange: fn(i32)
    pub handler:  NSObject
    pub init(ctl: NSSegmentedControl, handler: NSObject) {
        this.ctl = ctl
        this.titles  = []
        this.onChange = segmentedNoOp
        this.handler = handler
    }
}

fn segmentedNoOp(i: i32) {}

let SEGMENTEDS = new Map<i64, SegmentedSlot>()

class SegmentedHandler : NSObject {
    pub picked(sender: NSObject) {
        if let some(slot) = SEGMENTEDS.get(sender.handle) {
            let cb = slot.onChange
            cb(slot.ctl.selectedSegment() as i32)
        }
    }
}

pub fn createNativeSegmented(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let ctl = NSSegmentedControl.alloc().init(makeRect(x, y, width, height))
    ctl.setTrackingMode(NSSegmentSwitchTracking.selectOne)
    ctl.setEnabled(enabled)
    (ctl as NSView).setHidden(!visible)

    SegmentedHandler.register()
    let h = SegmentedHandler.alloc().init()
    ctl.setTarget(h)
    ctl.setAction(selectorPtr("picked:"))

    parent.addSubview(ctl as NSView)

    let handle = ctl.handle
    SEGMENTEDS[handle] = new SegmentedSlot(ctl, h as NSObject)
    handle
}

pub fn segmentedAddSegment(handle: i64, title: string) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        slot.titles.push(title)
        slot.ctl.segmentCount = slot.titles.length
        slot.ctl.setLabel(nsstr(title), (slot.titles.length - 1 as i64))
    }
}
pub fn segmentedClearSegments(handle: i64) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        slot.titles = []
        slot.ctl.segmentCount = 0 as i64
    }
}
pub fn segmentedSetTitle(handle: i64, index: i32, title: string) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        let n = slot.titles.length
        let i = index as i64
        if i >= 0 as i64 && i < n {
            slot.titles[i] = title
            slot.ctl.setLabel(nsstr(title), i)
        }
    }
}
pub fn segmentedGetTitle(handle: i64, index: i32): string {
    match SEGMENTEDS.get(handle) {
        some(slot) {
            let i = index as i64
            if i >= 0 as i64 && i < slot.titles.length {
                slot.titles[i]
            } else {
                ""
            }
        }
        none { "" }
    }
}
pub fn segmentedGetSelectedIndex(handle: i64): i32 {
    match SEGMENTEDS.get(handle) {
        some(slot) { slot.ctl.selectedSegment() as i32 }
        none       { -1 as i32 }
    }
}
pub fn segmentedSetSelectedIndex(handle: i64, index: i32) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        let i = index as i64
        if i >= 0 as i64 && i < slot.titles.length {
            slot.ctl.setSelected(true, i)
        }
    }
}
pub fn segmentedGetCount(handle: i64): i32 {
    match SEGMENTEDS.get(handle) {
        some(slot) { slot.titles.length as i32 }
        none       { 0 as i32 }
    }
}
pub fn setSegmentedFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        (slot.ctl as NSView).setFrame(makeRect(x, y, width, height))
    }
}
pub fn setSegmentedEnabled(handle: i64, enabled: bool) {
    if let some(slot) = SEGMENTEDS.get(handle) { slot.ctl.setEnabled(enabled) }
}
pub fn setSegmentedVisible(handle: i64, visible: bool) {
    if let some(slot) = SEGMENTEDS.get(handle) { (slot.ctl as NSView).setHidden(!visible) }
}
pub fn setSegmentedTooltip(handle: i64, text: string) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        if text == "" {
            (slot.ctl as NSView).setToolTip(nil)
        } else {
            (slot.ctl as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}
pub fn setOnSegmentedChange(handle: i64, cb: fn(i32)) {
    if let some(slot) = SEGMENTEDS.get(handle) { slot.onChange = cb }
}
