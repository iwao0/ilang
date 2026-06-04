/// Status bar — macOS doesn't ship a native status-bar control,
/// so we lay out a horizontal strip of `NSTextField`s ourselves
/// and dock it to the bottom of the window's contentView. The
/// strip's autoresizing mask keeps it pinned to the bottom edge
/// across window resizes; an `NSView` subclass takes over child
/// layout so each segment stays equal-width.

use cocoa {
    NSObject, NSView, NSTextField, NSRect, NSSize,
    NSAutoresizingMask,
    nsstr, ilstr, makeRect, makeSize,
}
use core { WINDOWS }

const STATUS_HEIGHT: f64 = 22.0

pub class StatusBarSlot {
    pub container: NSView
    pub fields:    NSTextField[]
    pub winHandle: i64
    pub init(container: NSView, fields: NSTextField[], winHandle: i64) {
        this.container = container
        this.fields    = fields
        this.winHandle = winHandle
    }
}

pub let STATUS_BARS = new Map<i64, StatusBarSlot>()

/// Equal-split layout: each field gets `width / N` and the row
/// fills the container left-to-right. Called both at construction
/// time and from `resizeSubviewsWithOldSize:` after window resize.
fn layoutFields(fields: NSTextField[], totalWidth: f64) {
    let n = fields.length
    if n == 0 as i64 { return }
    let segW = totalWidth / (n as f64)
    let i = 0 as i64
    while i < n {
        (fields[i] as NSView).setFrame(
            makeRect(segW * (i as f64), 0.0, segW, STATUS_HEIGHT)
        )
        i = i + 1
    }
}

/// `NSView` subclass that re-runs the equal-split layout every
/// time its own frame changes. Without this, the fields stick at
/// whatever size they were built with and the strip looks broken
/// after the user resizes the window.
pub class StatusBarContainer : NSView {
    pub override resizeSubviewsWithOldSize(oldSize: NSSize) {
        if let some(slot) = STATUS_BARS.get(this.handle) {
            layoutFields(slot.fields, this.frame.size.width)
        }
    }
}

@extern(C) {

fn buildField(text: string, x: f64, w: f64): NSTextField {
    let f = NSTextField.alloc().init(makeRect(x, 0.0, w, STATUS_HEIGHT))
    f.stringValue = nsstr(text)
    f.setEditable(false)
    f.setSelectable(false)
    f.setBezeled(true)
    f.setDrawsBackground(true)
    f
}

pub fn createNativeStatusBar(windowHandle: i64, segments: string[]): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let cv = winSlot.win.contentView
    let cvFrame = cv.frame
    let totalW = cvFrame.size.width
    let totalH = cvFrame.size.height

    StatusBarContainer.register()
    let container = StatusBarContainer.alloc().init()
    // GuiView (Window's contentView) is flipped — y=0 is the top,
    // so docking to the bottom means y = totalH - STATUS_HEIGHT.
    (container as NSView).setFrame(
        makeRect(0.0, totalH - STATUS_HEIGHT, totalW, STATUS_HEIGHT)
    )
    // Width follows the window; in flipped coords, "minYMargin"
    // is the top margin — making it flexible pins the bar to the
    // bottom edge across resizes.
    (container as NSView).setAutoresizingMask(
        (NSAutoresizingMask.widthSizable as u64) |
        (NSAutoresizingMask.minYMargin   as u64)
    )

    let fields: NSTextField[] = []
    let n = segments.length
    let segW = if n > 0 as i64 { totalW / (n as f64) } else { 0.0 }
    let i = 0 as i64
    while i < n {
        let field = buildField(segments[i], segW * (i as f64), segW)
        (container as NSView).addSubview(field as NSView)
        fields.push(field)
        i = i + 1
    }
    cv.addSubview(container as NSView)

    let handle = (container as NSView).handle
    STATUS_BARS[handle] = new StatusBarSlot(
        container as NSView, fields, windowHandle
    )
    handle
}

pub fn setStatusBarSegments(handle: i64, segments: string[]) {
    if let some(slot) = STATUS_BARS.get(handle) {
        // Detach previous fields — they aren't reused because the
        // new segment count may not match the old one.
        let n = slot.fields.length
        let i = 0 as i64
        while i < n {
            (slot.fields[i] as NSView).removeFromSuperview()
            i = i + 1
        }
        let newFields: NSTextField[] = []
        let totalW = slot.container.frame.size.width
        let m = segments.length
        let segW = if m > 0 as i64 { totalW / (m as f64) } else { 0.0 }
        let j = 0 as i64
        while j < m {
            let field = buildField(segments[j], segW * (j as f64), segW)
            slot.container.addSubview(field as NSView)
            newFields.push(field)
            j = j + 1
        }
        slot.fields = newFields
    }
}

pub fn setStatusBarSegmentText(handle: i64, index: i32, text: string) {
    if let some(slot) = STATUS_BARS.get(handle) {
        let i = index as i64
        if i < 0 as i64 || i >= slot.fields.length { return }
        slot.fields[i].stringValue = nsstr(text)
    }
}

} // @extern(C)
