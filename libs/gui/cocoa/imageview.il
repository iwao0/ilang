/// Image display widget — NSImageView fed an `NSImage` loaded
/// from the user-supplied filesystem path. Stretches to fill
/// the view's frame (NSImageScaleAxesIndependently = 1).

use cocoa {
    NSObject, NSView, NSWindow, NSEvent,
    NSImage, NSImageView,
    NSTrackingArea, NSTrackingAreaOptions,
    nsstr, nil, makeRect,
    classPtr, setObjectClass,
}
use core {
    WINDOWS, resolveParentView,
    noOpEnterLeave, noOpClickMod, noOpWheel,
    modCtrl, modShift, modAlt, modMeta,
}

class ImageViewSlot {
    pub view: NSImageView
    pub onMouseEnter:  fn(f64, f64)
    pub onMouseLeave:  fn(f64, f64)
    pub onMouseDown:   fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:     fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel:  fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub init(view: NSImageView) {
        this.view = view
        this.onMouseEnter  = noOpEnterLeave
        this.onMouseLeave  = noOpEnterLeave
        this.onMouseDown   = noOpClickMod
        this.onMouseUp     = noOpClickMod
        this.onContextMenu = noOpClickMod
        this.onMouseWheel  = noOpWheel
    }
}

let IMAGE_VIEWS = new Map<i64, ImageViewSlot>()

/// NSImageView subclass installed via isa-swap so AppKit's
/// mouse responder selectors land in our IMPs.
class FocusImageView : NSImageView {
    pub override mouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = IMAGE_VIEWS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let flags = e.modifierFlags as u64
            let cb = slot.onMouseDown
            cb(p.x, p.y, 0,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.mouseDown(event)
    }
    pub override mouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = IMAGE_VIEWS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let flags = e.modifierFlags as u64
            let cb = slot.onMouseUp
            cb(p.x, p.y, 0,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.mouseUp(event)
    }
    pub override rightMouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = IMAGE_VIEWS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let flags = e.modifierFlags as u64
            let ck = modCtrl(flags)
            let sk = modShift(flags)
            let ak = modAlt(flags)
            let mk = modMeta(flags)
            let down = slot.onMouseDown
            down(p.x, p.y, 2, ck, sk, ak, mk)
            let ctx = slot.onContextMenu
            ctx(p.x, p.y, 2, ck, sk, ak, mk)
        }
        super.rightMouseDown(event)
    }
    pub override rightMouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = IMAGE_VIEWS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let flags = e.modifierFlags as u64
            let cb = slot.onMouseUp
            cb(p.x, p.y, 2,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.rightMouseUp(event)
    }
    pub override otherMouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = IMAGE_VIEWS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let flags = e.modifierFlags as u64
            let cb = slot.onMouseDown
            cb(p.x, p.y, 1,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.otherMouseDown(event)
    }
    pub override otherMouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = IMAGE_VIEWS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let flags = e.modifierFlags as u64
            let cb = slot.onMouseUp
            cb(p.x, p.y, 1,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.otherMouseUp(event)
    }
    pub override mouseEntered(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = IMAGE_VIEWS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let cb = slot.onMouseEnter
            cb(p.x, p.y)
        }
    }
    pub override mouseExited(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = IMAGE_VIEWS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let cb = slot.onMouseLeave
            cb(p.x, p.y)
        }
    }
    pub scrollWheel(event: NSEvent) {
        if let some(slot) = IMAGE_VIEWS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(event.locationInWindow, nil)
            let flags = event.modifierFlags as u64
            let cb = slot.onMouseWheel
            cb(event.deltaX, event.deltaY, p.x, p.y,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
}

pub fn createNativeImageView(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let iv = NSImageView.alloc().init(makeRect(x, y, width, height))
    // Stretch to fit the frame — matches the Win32 / GTK default.
    iv.setImageScaling(1 as i64)
    (iv as NSView).setHidden(!visible)

    FocusImageView.register()
    let focusCls = classPtr("gui_impl.FocusImageView")
    if focusCls != 0 as i64 {
        let _ = setObjectClass((iv as NSView).handle, focusCls)
    }

    let opts = NSTrackingAreaOptions.mouseEnteredAndExited
             | NSTrackingAreaOptions.activeAlways
             | NSTrackingAreaOptions.inVisibleRect
    let area = NSTrackingArea.alloc().init(
        makeRect(0.0, 0.0, width, height), opts,
        iv as NSObject, nil
    )
    (iv as NSView).addTrackingArea(area as NSObject)

    parent.addSubview(iv as NSView)

    let handle = (iv as NSView).handle
    IMAGE_VIEWS[handle] = new ImageViewSlot(iv)
    let _ = enabled   // NSImageView has no enabled state — surface omitted.
    handle
}

pub fn setImageViewSrc(handle: i64, path: string) {
    if let some(slot) = IMAGE_VIEWS.get(handle) {
        if path == "" {
            slot.view.image = NSImage.__wrap_handle_unowned(0 as i64)
        } else {
            let img = NSImage.alloc().initWithContentsOfFile(nsstr(path))
            slot.view.image = img
        }
    }
}
pub fn setImageViewFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = IMAGE_VIEWS.get(handle) {
        (slot.view as NSView).setFrame(makeRect(x, y, width, height))
    }
}
pub fn setImageViewEnabled(handle: i64, enabled: bool) {
    // NSImageView inherits no `setEnabled:` selector worth wiring;
    // surface omitted on macOS for now.
}
pub fn setImageViewVisible(handle: i64, visible: bool) {
    if let some(slot) = IMAGE_VIEWS.get(handle) {
        (slot.view as NSView).setHidden(!visible)
    }
}
pub fn setImageViewTooltip(handle: i64, text: string) {
    if let some(slot) = IMAGE_VIEWS.get(handle) {
        if text == "" {
            (slot.view as NSView).setToolTip(nil)
        } else {
            (slot.view as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}

pub fn setOnImageViewMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnImageViewMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnImageViewMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnImageViewMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnImageViewContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnImageViewMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = IMAGE_VIEWS.get(handle) { slot.onMouseWheel = cb }
}
