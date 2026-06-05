/// LinkLabel. Borderless NSTextField styled in the system link
/// colour with `mouseDown:` overridden to fire the click callback
/// and hand the URL off to `NSWorkspace.openURL`.

use cocoa {
    NSObject, NSTextField, NSColor, NSView, NSEvent,
    NSTrackingArea, NSTrackingAreaOptions, NSCursor,
    nsstr, nil, makeRect,
    classPtr, setObjectClass,
    openURL,
}
use core { resolveParentView }

class LinkLabelSlot {
    pub label: NSTextField
    pub url:   string
    pub onClick: fn(f64, f64)
    pub init(label: NSTextField) {
        this.label = label
        this.url   = ""
        this.onClick = linkLabelNoOp
    }
}

fn linkLabelNoOp(x: f64, y: f64) {}

let LINKLABELS = new Map<i64, LinkLabelSlot>()

/// NSTextField subclass — captures mouseDown, fires `onClick`,
/// and (if a URL is set) hands it to LaunchServices. Isa-swapped
/// onto the field at creation time, same pattern as `FocusLabel`.
class LinkTextField : NSTextField {
    pub override mouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
        let y = (this as NSView).frame.size.height - p.y
        if let some(slot) = LINKLABELS.get(this.handle) {
            slot.onClick(p.x, y)
            if slot.url != "" {
                openURL(slot.url)
            }
        }
    }
    pub override mouseEntered(event: NSObject) {
        NSCursor.pointingHand().push()
    }
    pub override mouseExited(event: NSObject) {
        NSCursor.pop()
    }
}

pub fn createNativeLinkLabel(
    windowHandle: i64,
    text: string, url: string,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let tf = NSTextField.alloc().init(makeRect(x, y, width, height))
    tf.setEditable(false)
    tf.setBordered(false)
    tf.setBezeled(false)
    tf.setDrawsBackground(false)
    tf.setSelectable(false)
    tf.setEnabled(enabled)
    tf.stringValue = nsstr(text)
    // System link colour (#007AFF) — AppKit's `linkColor` would be
    // ideal but isn't surfaced in the current NSColor binding.
    tf.setTextColor(NSColor.rgba(0.0, 0.478, 1.0, 1.0) as NSObject)
    (tf as NSView).setHidden(!visible)

    LinkTextField.register()
    let linkCls = classPtr("gui_impl.LinkTextField")
    if linkCls != 0 as i64 {
        let _ = setObjectClass((tf as NSView).handle, linkCls)
    }

    // Tracking area for pointer-hand cursor on hover.
    let opts = NSTrackingAreaOptions.mouseEnteredAndExited
             | NSTrackingAreaOptions.activeAlways
             | NSTrackingAreaOptions.inVisibleRect
    let area = NSTrackingArea.alloc().init(
        makeRect(0.0, 0.0, width, height), opts,
        tf as NSObject, nil
    )
    (tf as NSView).addTrackingArea(area as NSObject)

    parent.addSubview(tf as NSView)

    let handle = tf.handle
    let slot = new LinkLabelSlot(tf)
    slot.url = url
    LINKLABELS[handle] = slot
    handle
}

pub fn setLinkLabelText(handle: i64, text: string) {
    if let some(slot) = LINKLABELS.get(handle) {
        slot.label.stringValue = nsstr(text)
    }
}
pub fn setLinkLabelUrl(handle: i64, url: string) {
    if let some(slot) = LINKLABELS.get(handle) { slot.url = url }
}
pub fn setLinkLabelFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = LINKLABELS.get(handle) {
        (slot.label as NSView).setFrame(makeRect(x, y, width, height))
    }
}
pub fn setLinkLabelEnabled(handle: i64, enabled: bool) {
    if let some(slot) = LINKLABELS.get(handle) { slot.label.setEnabled(enabled) }
}
pub fn setLinkLabelVisible(handle: i64, visible: bool) {
    if let some(slot) = LINKLABELS.get(handle) { (slot.label as NSView).setHidden(!visible) }
}
pub fn setLinkLabelTooltip(handle: i64, text: string) {
    if let some(slot) = LINKLABELS.get(handle) {
        if text == "" {
            (slot.label as NSView).setToolTip(nil)
        } else {
            (slot.label as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}
pub fn setOnLinkLabelClick(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = LINKLABELS.get(handle) { slot.onClick = cb }
}
