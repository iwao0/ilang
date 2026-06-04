/// Multi-line text input. `NSTextView` hosted inside an
/// `NSScrollView` so the user gets the standard scroll-bar
/// behaviour for content overflowing the visible area.
///
/// Handle = NSTextView's ObjC pointer. The scroll view is stored
/// on the slot so size / visibility setters can drive the outer
/// frame while the textDidChange delegate / FocusTextView event
/// overrides talk to the text view directly.

use cocoa {
    NSObject, NSView, NSWindow,
    NSText, NSTextView, NSScrollView,
    NSBorderType, NSEvent, NSNotification,
    NSTrackingArea, NSTrackingAreaOptions,
    nsstr, ilstr, makeRect, nil,
    classPtr, setObjectClass,
}
use core {
    WINDOWS, resolveParentView, noOpText, noOp1, noOpKey,
    noOpEnterLeave, noOpClickMod, noOpWheel,
    keyCodeToString, modCtrl, modShift, modAlt, modMeta,
}

class TextAreaSlot {
    pub scroll:   NSScrollView
    pub textView: NSTextView
    pub onChange: fn(string)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub onMouseEnter:  fn(f64, f64)
    pub onMouseLeave:  fn(f64, f64)
    pub onMouseDown:   fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:     fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onKeyDown:     fn(string, bool, bool, bool, bool)
    pub onKeyUp:       fn(string, bool, bool, bool, bool)
    pub onMouseWheel:  fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub delegate: NSObject
    pub init(scroll: NSScrollView, textView: NSTextView, delegate: NSObject) {
        this.scroll   = scroll
        this.textView = textView
        this.delegate = delegate
        this.onChange = noOpText
        this.onFocus  = noOp1
        this.onBlur   = noOp1
        this.onMouseEnter  = noOpEnterLeave
        this.onMouseLeave  = noOpEnterLeave
        this.onMouseDown   = noOpClickMod
        this.onMouseUp     = noOpClickMod
        this.onContextMenu = noOpClickMod
        this.onKeyDown     = noOpKey
        this.onKeyUp       = noOpKey
        this.onMouseWheel  = noOpWheel
    }
}

let TEXT_AREAS = new Map<i64, TextAreaSlot>()

/// NSTextView subclass installed via isa-swap so AppKit's
/// `becomeFirstResponder` / `resignFirstResponder` and the mouse
/// / key responder selectors land in our IMPs. Same pattern as
/// FocusTextField — see cocoa/edit.il for the design notes on
/// the isa-swap and `super`-trampoline caveats.
class FocusTextView : NSTextView {
    pub becomeFirstResponder(): bool {
        if let some(slot) = TEXT_AREAS.get(this.handle) {
            let cb = slot.onFocus
            cb()
        }
        super.becomeFirstResponder()
    }
    pub resignFirstResponder(): bool {
        if let some(slot) = TEXT_AREAS.get(this.handle) {
            let cb = slot.onBlur
            cb()
        }
        super.resignFirstResponder()
    }
    pub override mouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
        let flags = e.modifierFlags as u64
        if let some(slot) = TEXT_AREAS.get(this.handle) {
            let cb = slot.onMouseDown
            cb(p.x, p.y, 0,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.mouseDown(event)
    }
    pub override mouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = TEXT_AREAS.get(this.handle) {
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
        if let some(slot) = TEXT_AREAS.get(this.handle) {
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
        if let some(slot) = TEXT_AREAS.get(this.handle) {
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
        if let some(slot) = TEXT_AREAS.get(this.handle) {
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
        if let some(slot) = TEXT_AREAS.get(this.handle) {
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
        if let some(slot) = TEXT_AREAS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let cb = slot.onMouseEnter
            cb(p.x, p.y)
        }
    }
    pub override mouseExited(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = TEXT_AREAS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let cb = slot.onMouseLeave
            cb(p.x, p.y)
        }
    }
    pub scrollWheel(event: NSEvent) {
        if let some(slot) = TEXT_AREAS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(event.locationInWindow, nil)
            let flags = event.modifierFlags as u64
            let cb = slot.onMouseWheel
            cb(event.deltaX, event.deltaY, p.x, p.y,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        // Forward so the scroll view's own scrolling still works.
        super.scrollWheel(event)
    }
    pub keyDown(event: NSEvent) {
        if let some(slot) = TEXT_AREAS.get(this.handle) {
            let flags = event.modifierFlags as u64
            let cb = slot.onKeyDown
            cb(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        // Forward so character input + arrow-key caret movement
        // keep working.
        super.keyDown(event)
    }
    pub keyUp(event: NSEvent) {
        if let some(slot) = TEXT_AREAS.get(this.handle) {
            let flags = event.modifierFlags as u64
            let cb = slot.onKeyUp
            cb(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.keyUp(event)
    }
}

/// `NSTextDelegate` target. `textDidChange:` fires on every
/// keystroke; we read the buffer back through `string` and feed
/// the latest snapshot to the user callback.
class TextAreaDelegate : NSObject {
    pub textDidChange(n: NSNotification) {
        let obj = n.notificationObject
        if let some(slot) = TEXT_AREAS.get(obj.handle) {
            let s = ilstr(slot.textView.string)
            let cb = slot.onChange
            cb(s)
        }
    }
}

pub fn createNativeTextArea(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }

    let scroll = NSScrollView.alloc().init(makeRect(x, y, width, height))
    scroll.setHasVerticalScroller(true)
    scroll.setHasHorizontalScroller(false)
    scroll.setAutohidesScrollers(true)
    scroll.setBorderType(NSBorderType.bezelBorder)
    (scroll as NSView).setHidden(!visible)

    let tv = NSTextView.alloc().init(makeRect(0.0, 0.0, width, height))
    tv.string = nsstr(text)
    tv.setRichText(false)
    // ilang doesn't widen NSTextView to its NSText parent for
    // selector lookup, so re-wrap the same handle as NSText to
    // reach the inherited setEditable / setSelectable / setDelegate.
    let tvAsText = NSText.__wrap_handle_unowned((tv as NSView).handle)
    tvAsText.setEditable(enabled)
    tvAsText.setSelectable(true)

    // Isa-swap the text view onto FocusTextView so AppKit's
    // responder dispatch finds our IMP overrides above.
    FocusTextView.register()
    let focusCls = classPtr("gui_impl.FocusTextView")
    if focusCls != 0 as i64 {
        let _ = setObjectClass((tv as NSView).handle, focusCls)
    }

    // Delegate for textDidChange:. Stored on the slot so the
    // ObjC delegate target keeps a live ilang reference.
    TextAreaDelegate.register()
    let dg = TextAreaDelegate.alloc().init()
    tvAsText.setDelegate(dg as NSObject)

    // Tracking area for mouseEntered: / mouseExited:.
    let opts = NSTrackingAreaOptions.mouseEnteredAndExited
             | NSTrackingAreaOptions.activeAlways
             | NSTrackingAreaOptions.inVisibleRect
    let area = NSTrackingArea.alloc().init(
        makeRect(0.0, 0.0, width, height), opts,
        tv as NSObject, nil
    )
    (tv as NSView).addTrackingArea(area as NSObject)

    scroll.setDocumentView(tv as NSView)
    parent.addSubview(scroll as NSView)

    let handle = (tv as NSView).handle
    TEXT_AREAS[handle] = new TextAreaSlot(scroll, tv, dg as NSObject)
    handle
}

pub fn readTextAreaText(handle: i64): string {
    match TEXT_AREAS.get(handle) {
        some(slot) { ilstr(slot.textView.string) }
        none       { "" }
    }
}
pub fn setTextAreaText(handle: i64, text: string) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        slot.textView.string = nsstr(text)
    }
}
pub fn setTextAreaFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        (slot.scroll as NSView).setFrame(makeRect(x, y, width, height))
    }
}
pub fn setTextAreaEnabled(handle: i64, enabled: bool) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        NSText.__wrap_handle_unowned((slot.textView as NSView).handle)
            .setEditable(enabled)
    }
}
pub fn setTextAreaVisible(handle: i64, visible: bool) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        (slot.scroll as NSView).setHidden(!visible)
    }
}
pub fn setTextAreaTooltip(handle: i64, text: string) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        if text == "" {
            (slot.scroll as NSView).setToolTip(nil)
        } else {
            (slot.scroll as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}
pub fn setTextAreaReadOnly(handle: i64, readOnly: bool) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        NSText.__wrap_handle_unowned((slot.textView as NSView).handle)
            .setEditable(!readOnly)
    }
}
pub fn focusTextArea(handle: i64) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        let win = (slot.textView as NSView).window
        let _ = win.makeFirstResponder(slot.textView as NSObject)
    }
}
pub fn blurTextArea(handle: i64) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        let win = (slot.textView as NSView).window
        if win.firstResponder.handle == (slot.textView as NSView).handle {
            let _ = win.makeFirstResponder(nil)
        }
    }
}

pub fn setOnTextAreaChange(handle: i64, cb: fn(string)) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onChange = cb }
}
pub fn setOnTextAreaFocus(handle: i64, cb: fn()) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onFocus = cb }
}
pub fn setOnTextAreaBlur(handle: i64, cb: fn()) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onBlur = cb }
}
pub fn setOnTextAreaMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnTextAreaMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnTextAreaMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnTextAreaMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnTextAreaContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnTextAreaKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnTextAreaKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onKeyUp = cb }
}
pub fn setOnTextAreaMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onMouseWheel = cb }
}
