/// Panel — plain `NSView` host. The view is flipped so children's
/// `y` is measured from the top, matching the convention every
/// other gui-core widget uses.

use cocoa { NSView, makeRect }
use core { resolveParentView }

pub class FlippedPanelView : NSView {
    pub isFlipped(): bool { true }
}

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
    let view = FlippedPanelView.alloc().init(makeRect(x, y, width, height))
    view.setHidden(!visible)
    parent.addSubview(view as NSView)
    (view as NSView).handle
}

pub fn setPanelFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    let view = handle as NSView
    if view.handle == 0 as i64 { return }
    view.setFrame(makeRect(x, y, width, height))
}

pub fn setPanelVisible(handle: i64, visible: bool) {
    let view = handle as NSView
    if view.handle == 0 as i64 { return }
    view.setHidden(!visible)
}
