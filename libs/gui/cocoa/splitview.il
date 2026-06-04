/// Split view — `NSSplitView` with native splitter bars. Each
/// `addChild` allocates a flipped `SplitPaneView` and adds it as
/// a subview of the splitter; AppKit then draws a divider
/// between adjacent pane views and lets the user drag it.
///
/// Children of the SplitView parent under their own pane's
/// flipped `NSView`, so each pane has its own y-down coordinate
/// space (same convention as ScrollView's `FlippedDocumentView`).

use cocoa {
    NSObject, NSView, NSSplitView, NSRect,
    makeRect,
}
use core { WINDOWS, resolveParentView }

/// Per-pane flipped NSView; child widgets parent under it.
pub class SplitPaneView : NSView {
    pub isFlipped(): bool { true }
}

pub class SplitViewSlot {
    pub split: NSSplitView
    pub panes: NSView[]
    pub init(split: NSSplitView) {
        this.split = split
        let p: NSView[] = []
        this.panes = p
    }
}

pub let SPLIT_VIEWS = new Map<i64, SplitViewSlot>()

pub fn createNativeSplitView(
    parentHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool, orientation: i32
): i64 {
    let parent = match resolveParentView(parentHandle) {
        some(v) { v }
        none    { return -1 }
    }
    let split = NSSplitView.alloc().init(makeRect(x, y, width, height))
    // SplitOrientation.horizontal = 0  → panes side by side,
    // which AppKit calls "vertical" (vertical divider bars).
    split.isVertical = orientation == 0 as i32
    // Style 2 = thin pane splitter.
    split.setDividerStyle(2 as i64)
    (split as NSView).setHidden(!visible)
    let _ = enabled  // NSSplitView has no setEnabled: surface.

    parent.addSubview(split as NSView)

    SplitPaneView.register()
    let handle = (split as NSView).handle
    SPLIT_VIEWS[handle] = new SplitViewSlot(split)
    handle
}

pub fn splitViewDocumentHandle(handle: i64): i64 {
    match SPLIT_VIEWS.get(handle) {
        some(slot) {
            let n = slot.panes.length
            if n > 0 as i64 {
                slot.panes[n - 1 as i64].handle
            } else {
                (slot.split as NSView).handle
            }
        }
        none { -1 as i64 }
    }
}

/// Allocate a fresh pane view, add it as the splitter's next
/// subview, and re-balance. The returned handle becomes the
/// new child's `parent.nativeChildHandle()`, so the child's
/// `createNative*` call lands its widget inside the pane.
pub fn splitViewBeginNewPane(handle: i64): i64 {
    match SPLIT_VIEWS.get(handle) {
        some(slot) {
            let splitFrame = (slot.split as NSView).frame
            // Use a non-zero rect so AppKit doesn't collapse the
            // pane to zero — `adjustSubviews` below re-balances.
            let pane = SplitPaneView.alloc().init()
            (pane as NSView).setFrame(makeRect(
                0.0, 0.0, splitFrame.size.width, splitFrame.size.height
            ))
            slot.split.addSubview(pane as NSView)
            slot.panes.push(pane as NSView)
            slot.split.adjustSubviews()
            (pane as NSView).handle
        }
        none { -1 as i64 }
    }
}

pub fn setSplitViewFrame(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        (slot.split as NSView).setFrame(makeRect(x, y, w, h))
        slot.split.adjustSubviews()
    }
}

pub fn setSplitViewVisible(handle: i64, visible: bool) {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        (slot.split as NSView).setHidden(!visible)
    }
}

pub fn setSplitViewEnabled(handle: i64, enabled: bool) {
    // NSSplitView has no native enabled state.
}

pub fn setSplitViewOrientation(handle: i64, orientation: i32) {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        slot.split.isVertical = orientation == 0 as i32
        slot.split.adjustSubviews()
    }
}

pub fn setSplitViewPosition(handle: i64, dividerIndex: i32, px: f64) {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        slot.split.setPosition(px, dividerIndex as i64)
    }
}
