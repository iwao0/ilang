/// Scrollable container — `NSScrollView` with a flipped
/// `documentView` so child positions follow the same y-down
/// convention the rest of gui-core uses. The public `handle` is
/// the NSScrollView's raw pointer; the documentView's pointer is
/// returned by `scrollViewDocumentHandle` and used as the parent
/// of every child widget realised through `Container.addChild` →
/// `Control.attachTo(this)`.

use cocoa {
    NSObject, NSView, NSScrollView, NSSize, NSRect,
    NSBorderType,
    makeRect, makeSize,
}
use core { WINDOWS, resolveParentView }

/// `NSView` subclass with `isFlipped = true` so y=0 is the top
/// of the document. Without this, every child's `y` would mean
/// "distance from the bottom" — the opposite of the convention
/// every other widget already follows.
pub class FlippedDocumentView : NSView {
    pub isFlipped(): bool { true }
}

/// Per-ScrollView state, keyed in `SCROLL_VIEWS` by the
/// NSScrollView pointer (`handle`). `doc` is stored as a plain
/// `NSView` to dodge cross-file generic-type plumbing — the
/// flipped subclass is installed via `register()` once, then
/// every `alloc().init()` returns a value that's safe to widen.
pub class ScrollViewSlot {
    pub scroll: NSScrollView
    pub doc:    NSView
    pub init(scroll: NSScrollView, doc: NSView) {
        this.scroll = scroll
        this.doc    = doc
    }
}

let SCROLL_VIEWS = new Map<i64, ScrollViewSlot>()

pub fn createNativeScrollView(
    parentHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool,
    showHScroll: bool, showVScroll: bool
): i64 {
    let parent = match resolveParentView(parentHandle) {
        some(v) { v }
        none    { return -1 }
    }
    let scroll = NSScrollView.alloc().init(makeRect(x, y, width, height))
    scroll.setHasVerticalScroller(showVScroll)
    scroll.setHasHorizontalScroller(showHScroll)
    scroll.setBorderType(NSBorderType.noBorder)
    // Hide bars until the content actually overflows — same
    // behaviour as the win32 / GTK backends.
    scroll.setAutohidesScrollers(true)

    FlippedDocumentView.register()
    // Start the document the same size as the scroll view's
    // clip — `setScrollViewContentSize` widens it later as
    // children land.
    let doc = FlippedDocumentView.alloc().init()
    (doc as NSView).setFrame(makeRect(0.0, 0.0, width, height))
    scroll.setDocumentView(doc as NSView)
    (scroll as NSView).setHidden(!visible)
    let _ = enabled  // NSScrollView has no setEnabled: surface.

    parent.addSubview(scroll as NSView)

    let handle = (scroll as NSView).handle
    SCROLL_VIEWS[handle] = new ScrollViewSlot(scroll, doc as NSView)
    handle
}

/// Pointer of the inner documentView — what `Container.attachTo`
/// resolves to via `ScrollView.nativeChildHandle()`. Widgets see
/// it as a raw NSView* and parent under it through
/// `resolveParentView`.
pub fn scrollViewDocumentHandle(handle: i64): i64 {
    match SCROLL_VIEWS.get(handle) {
        some(slot) { slot.doc.handle }
        none       { -1 as i64 }
    }
}

pub fn setScrollViewFrame(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = SCROLL_VIEWS.get(handle) {
        (slot.scroll as NSView).setFrame(makeRect(x, y, w, h))
    }
}

pub fn setScrollViewVisible(handle: i64, visible: bool) {
    if let some(slot) = SCROLL_VIEWS.get(handle) {
        (slot.scroll as NSView).setHidden(!visible)
    }
}

pub fn setScrollViewEnabled(handle: i64, enabled: bool) {
    // NSScrollView has no setEnabled: API; surface omitted.
}

pub fn setScrollViewShowHScroll(handle: i64, show: bool) {
    if let some(slot) = SCROLL_VIEWS.get(handle) {
        slot.scroll.setHasHorizontalScroller(show)
    }
}

pub fn setScrollViewShowVScroll(handle: i64, show: bool) {
    if let some(slot) = SCROLL_VIEWS.get(handle) {
        slot.scroll.setHasVerticalScroller(show)
    }
}

/// Resize the documentView to `(w, h)` — this is the scrollable
/// area. Anything larger than the scroll view's frame becomes
/// reachable by scrolling; anything smaller / equal keeps the
/// scroll bars hidden (auto-hide is on).
pub fn setScrollViewContentSize(handle: i64, w: f64, h: f64) {
    if let some(slot) = SCROLL_VIEWS.get(handle) {
        // Origin stays at (0, 0) — the document view's coordinate
        // system anchors its top-left at the scroll view's
        // scrollable origin, so the content rectangle is just
        // `(0, 0, w, h)`.
        slot.doc.setFrame(makeRect(0.0, 0.0, w, h))
    }
}
