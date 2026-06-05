/// Panel — plain `NSView` host. The view is flipped so children's
/// `y` is measured from the top, matching the convention every
/// other gui-core widget uses.

use cocoa { NSView, makeRect }
use core { resolveParentView }

pub class FlippedPanelView : NSView {
    pub isFlipped(): bool { true }
}

/// Per-handle slot — owns the flipped NSView so `setPanel*` lookups
/// don't need to round-trip through an `i64 → NSView` cast (which
/// ilang's type system rejects: NSView isn't an `@handle` class).
/// Mirrors the pattern every other widget here uses (canvas /
/// groupbox / button / ...).
class PanelSlot {
    pub view: FlippedPanelView
    pub init(view: FlippedPanelView) { this.view = view }
}

let PANELS = new Map<i64, PanelSlot>()

pub fn createNativePanel(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(v) { v }
        none    { return -1 }
    }
    FlippedPanelView.register()
    // NSView has no `initWithFrame:` in the bindings — call the
    // 0-arg `init` from NSObject, then size via `setFrame:`.
    // Matches `CanvasView` in canvas.il.
    let view = FlippedPanelView.alloc().init()
    view.setFrame(makeRect(x, y, width, height))
    view.setHidden(!visible)
    parent.addSubview(view as NSView)
    let handle = (view as NSView).handle
    PANELS[handle] = new PanelSlot(view)
    handle
}

pub fn setPanelFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = PANELS.get(handle) {
        slot.view.setFrame(makeRect(x, y, width, height))
    }
}

pub fn setPanelVisible(handle: i64, visible: bool) {
    if let some(slot) = PANELS.get(handle) {
        slot.view.setHidden(!visible)
    }
}
