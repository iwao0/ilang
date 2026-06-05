/// ColorWell. `NSColorWell` with target/action plumbed in,
/// `setContinuous(false)` so `onChange` fires once per commit
/// rather than every drag delta in the floating colour panel.

use cocoa {
    NSObject, NSColor, NSColorWell, NSColorPanel, NSView,
    NSNotificationCenter,
    nsstr, nil, makeRect, selectorPtr,
}
use core { resolveParentView }

class ColorWellSlot {
    pub well:         NSColorWell
    pub onChange:     fn(u32)
    pub handler:      NSObject
    /// Last colour the continuous handler saw, latched so we can
    /// emit it once on panel close instead of streaming every
    /// drag delta to user code.
    pub pendingColor: u32
    pub hasPending:   bool
    pub init(well: NSColorWell, handler: NSObject) {
        this.well = well
        this.onChange = colorWellNoOp
        this.handler = handler
        this.pendingColor = 0 as u32
        this.hasPending = false
    }
}

fn colorWellNoOp(c: u32) {}

let COLORWELLS = new Map<i64, ColorWellSlot>()

fn argbToWellColor(argb: u32): NSColor {
    let mask: u32 = 0xFF as u32
    let a = (((argb >> 24) & mask) as f64) / 255.0
    let r = (((argb >> 16) & mask) as f64) / 255.0
    let g = (((argb >>  8) & mask) as f64) / 255.0
    let b = (( argb        & mask) as f64) / 255.0
    NSColor.rgba(r, g, b, a)
}

fn wellColorToArgb(c: NSColor): u32 {
    let r = (c.redComponent   * 255.0) as u32
    let g = (c.greenComponent * 255.0) as u32
    let b = (c.blueComponent  * 255.0) as u32
    let a = (c.alphaComponent * 255.0) as u32
    (a << 24) | (r << 16) | (g << 8) | b
}

class ColorWellHandler : NSObject {
    pub picked(sender: NSObject) {
        if let some(slot) = COLORWELLS.get(sender.handle) {
            // Latch the new colour; the real `onChange` fires on
            // panel-close (see `ensurePanelObserver`).
            slot.pendingColor = wellColorToArgb(slot.well.color)
            slot.hasPending   = true
        }
    }
}

let PANEL_OBSERVER_INSTALLED: bool = false

fn flushPendingColors() {
    for slot in COLORWELLS.values() {
        if slot.hasPending {
            slot.hasPending = false
            slot.onChange(slot.pendingColor)
        }
    }
}

fn ensurePanelObserver() {
    if PANEL_OBSERVER_INSTALLED { return }
    PANEL_OBSERVER_INSTALLED = true
    let nc = NSNotificationCenter.defaultCenter()
    let panel = NSColorPanel.sharedColorPanel()
    let _ = nc.addObserver(
        nsstr("NSWindowWillCloseNotification"),
        panel as NSObject,
        nil,
        fn(notification: i64) {
            flushPendingColors()
        }
    )
}

pub fn createNativeColorWell(
    windowHandle: i64,
    initial: u32,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let cw = NSColorWell.alloc().init(makeRect(x, y, width, height))
    cw.setColor(argbToWellColor(initial))
    cw.setEnabled(enabled)
    // setContinuous(true) — modern macOS skips the target/action
    // on NSColorWell unless continuous tracking is enabled. The
    // public API still promises "fires on commit", so the handler
    // debounces the burst of in-drag callbacks into a single
    // post-commit emission via lastEmittedColor.
    cw.setContinuous(true)
    (cw as NSView).setHidden(!visible)

    ColorWellHandler.register()
    let h = ColorWellHandler.alloc().init()
    cw.setTarget(h as NSObject)
    cw.setAction(selectorPtr("picked:"))

    parent.addSubview(cw as NSView)

    let handle = cw.handle
    COLORWELLS[handle] = new ColorWellSlot(cw, h as NSObject)
    ensurePanelObserver()
    handle
}

pub fn readColorWellColor(handle: i64): u32 {
    match COLORWELLS.get(handle) {
        some(slot) { wellColorToArgb(slot.well.color) }
        none       { 0 as u32 }
    }
}
pub fn setColorWellColor(handle: i64, color: u32) {
    if let some(slot) = COLORWELLS.get(handle) { slot.well.setColor(argbToWellColor(color)) }
}
pub fn setColorWellFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = COLORWELLS.get(handle) {
        (slot.well as NSView).setFrame(makeRect(x, y, width, height))
    }
}
pub fn setColorWellEnabled(handle: i64, enabled: bool) {
    if let some(slot) = COLORWELLS.get(handle) { slot.well.setEnabled(enabled) }
}
pub fn setColorWellVisible(handle: i64, visible: bool) {
    if let some(slot) = COLORWELLS.get(handle) { (slot.well as NSView).setHidden(!visible) }
}
pub fn setColorWellTooltip(handle: i64, text: string) {
    if let some(slot) = COLORWELLS.get(handle) {
        if text == "" {
            (slot.well as NSView).setToolTip(nil)
        } else {
            (slot.well as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}
pub fn setOnColorWellChange(handle: i64, cb: fn(u32)) {
    if let some(slot) = COLORWELLS.get(handle) { slot.onChange = cb }
}
