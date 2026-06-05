/// Single-line text input. NSTextField in editable mode; the
/// delegate hop on `controlTextDidChange:` forwards the new
/// contents to the registered callback.

use cocoa {
    NSObject, NSTextField, NSText, NSView, NSWindow,
    NSEvent, NSEventMask, NSNotification,
    NSTrackingArea, NSTrackingAreaOptions,
    NSRange,
    nsstr, ilstr, makeRect, makeTextField, nil,
    classPtr, setObjectClass, selectorPtr,
}
use core {
    WINDOWS, resolveParentView, noOpText, noOp1, noOpKey,
    noOpEnterLeave, noOpClickMod, noOpWheel,
    keyCodeToString, modCtrl, modShift, modAlt, modMeta,
}

class EditSlot {
    pub edit:     NSTextField
    pub onChange: fn(string)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onKeyDown:    fn(string, bool, bool, bool, bool)
    pub onKeyUp:      fn(string, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)
    /// Retained handle to the `+addLocalMonitorForEventsMatchingMask:`
    /// monitor that surfaces key events. Stored on the slot so the
    /// monitor's lifetime tracks the text field's; left alive until
    /// process exit (matches the rest of libs/gui's "leak until app
    /// quit" widget lifetime).
    pub keyMonitor: NSObject
    pub handler:  NSObject
    /// Character cap. `0` = no limit. AppKit has no built-in max
    /// length on NSTextField; the change-handler reads this and
    /// truncates `stringValue` whenever a notification arrives
    /// with a string longer than the cap.
    pub maxLength: i32
    pub init(edit: NSTextField, handler: NSObject) {
        this.edit = edit
        this.onChange = noOpText
        this.onFocus  = noOp1
        this.onBlur   = noOp1
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseDown  = noOpClickMod
        this.onMouseUp    = noOpClickMod
        this.onContextMenu = noOpClickMod
        this.onKeyDown = noOpKey
        this.onKeyUp   = noOpKey
        this.onMouseWheel = noOpWheel
        this.keyMonitor = nil
        this.handler = handler
        this.maxLength = 0 as i32
    }
}


let EDITS = new Map<i64, EditSlot>()

/// NSTextField subclass installed via isa-swap so AppKit's
/// `becomeFirstResponder` dispatch lands in our IMP. Mirrors the
/// FocusButton trick — `controlTextDidBeginEditing:` only fires
/// once editing actually opens, which `makeFirstResponder:`
/// doesn't always trigger, so we hook the responder transition
/// directly. We return `true` to accept the role and let AppKit's
/// default machinery substitute the field editor as it normally
/// would.
class FocusTextField : NSTextField {
    pub becomeFirstResponder(): bool {
        if let some(slot) = EDITS.get(this.handle) {
            slot.onFocus()
        }
        // Forward to NSTextField's own becomeFirstResponder so the
        // field editor actually gets installed — without that
        // `currentEditor` stays nil and `setSelectedRange:` has
        // nowhere to write. The super-dispatch fix from
        // 671d5458 makes this call safe (was the same SIGSEGV
        // path the old workaround had to dodge).
        super.becomeFirstResponder()
    }
    /// `mouseDown:` / `mouseUp:` etc. forward the event after
    /// firing our Signal so NSTextField's default machinery —
    /// click-to-position-cursor, double-click word selection,
    /// drag-to-select — keeps working. The ilang ObjC-super
    /// trampoline crashed for `bool`-returning overrides on
    /// FocusButton; void-returning `mouseDown:` is the first
    /// case where we exercise the same path with the suspected-
    /// safe return shape.
    pub override mouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
        let y = (this as NSView).frame.size.height - p.y
        let flags = e.modifierFlags as u64
        let ck = modCtrl(flags)
        let sk = modShift(flags)
        let ak = modAlt(flags)
        let mk = modMeta(flags)
        if let some(slot) = EDITS.get(this.handle) {
            slot.onMouseDown(p.x, y, 0, ck, sk, ak, mk)
        }
        super.mouseDown(event)
        // `super.mouseDown:` enters NSCell.trackMouse:..untilMouseUp:
        // which spins its own `[NSApp nextEventMatchingMask:]` loop
        // until the user releases — handling drag-select natively in
        // the process — then returns. AppKit consumes that mouseUp
        // inside trackMouse and never re-dispatches it to `mouseUp:`,
        // so the override below never fires for left clicks. To keep
        // the public API symmetric (onMouseDown without an onMouseUp
        // would be a footgun), synthesise the emission once super
        // returns. Position carries the press point — the release
        // event is gone by then and recomputing from
        // `[NSEvent mouseLocation]` would need a screen→window→view
        // conversion that buys little for most click semantics.
        if let some(slot) = EDITS.get(this.handle) {
            let up = slot.onMouseUp
            up(p.x, y, 0, ck, sk, ak, mk)
        }
    }
    pub override mouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = EDITS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            slot.onMouseUp(p.x, y, 0,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.mouseUp(event)
    }
    pub override rightMouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = EDITS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            let ck = modCtrl(flags)
            let sk = modShift(flags)
            let ak = modAlt(flags)
            let mk = modMeta(flags)
            let down = slot.onMouseDown
            down(p.x, y, 2, ck, sk, ak, mk)
            let ctx = slot.onContextMenu
            ctx(p.x, y, 2, ck, sk, ak, mk)
        }
        super.rightMouseDown(event)
    }
    pub override rightMouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = EDITS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            slot.onMouseUp(p.x, y, 2,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.rightMouseUp(event)
    }
    pub override otherMouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = EDITS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            slot.onMouseDown(p.x, y, 1,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.otherMouseDown(event)
    }
    pub override otherMouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = EDITS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            slot.onMouseUp(p.x, y, 1,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.otherMouseUp(event)
    }
    pub override mouseEntered(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = EDITS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            slot.onMouseEnter(p.x, y)
        }
    }
    pub override mouseExited(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = EDITS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            slot.onMouseLeave(p.x, y)
        }
    }
    /// Wheel scroll over the field. NSResponder declares the
    /// selector so no `@objc` decoration is needed — the
    /// auto-lift infers `scrollWheel:` from name + arity.
    pub scrollWheel(event: NSEvent) {
        if let some(slot) = EDITS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(event.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = event.modifierFlags as u64
            slot.onMouseWheel(event.deltaX, event.deltaY, p.x, y,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
}

class ChangeHandler : NSObject {
    /// Edit-session-end notification. Fires when the field editor
    /// commits (user Tabs away, clicks elsewhere, presses Return,
    /// etc.). Maps cleanly to the public `onBlur`. The focus-in
    /// side rides on `FocusTextField.becomeFirstResponder`
    /// instead because programmatic `makeFirstResponder:` doesn't
    /// always trigger `controlTextDidBeginEditing:` — Apple ties
    /// that notification to the moment editing actually opens,
    /// not pure focus transitions.
    pub controlTextDidEndEditing(n: NSNotification) {
        if let some(slot) = EDITS.get(n.notificationObject.handle) {
            slot.onBlur()
        }
    }
    pub controlTextDidChange(n: NSNotification) {
        if let some(slot) = EDITS.get(n.notificationObject.handle) {
            let s = ilstr(slot.edit.stringValue)
            // Honour `maxLength` here so a too-long string never
            // reaches the user callback. Truncate, write back, then
            // surface the trimmed value as if the user typed it.
            // The write-back fires another `controlTextDidChange:`
            // but the length is now within the cap so the second
            // pass is a no-op.
            let cap = slot.maxLength
            if cap as i64 > 0 as i64 && s.length > cap as i64 {
                let trimmed = s.slice(0 as i64, cap as i64)
                slot.edit.stringValue = nsstr(trimmed)
                slot.onChange(trimmed)
                return
            }
            slot.onChange(s)
        }
    }
}

pub fn createNativeEdit(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let tf = makeTextField(text, x, y, width, height)
    tf.setEnabled(enabled)
    tf.setHidden(!visible)
    // Swap the freshly-created NSTextField over to FocusTextField
    // so AppKit dispatches `becomeFirstResponder` through our IMP.
    // See the FocusButton path in cocoa/button.il for the fully-
    // qualified class-name caveat — same fragile lookup.
    FocusTextField.register()
    let focusCls = classPtr("gui_impl.FocusTextField")
    if focusCls != 0 as i64 {
        let _ = setObjectClass((tf as NSView).handle, focusCls)
    }
    // Tracking area for mouseEntered: / mouseExited:. The
    // `inVisibleRect` option pins the area to the view's bounds
    // so a later setFrame doesn't desync the hit region.
    let opts = NSTrackingAreaOptions.mouseEnteredAndExited
             | NSTrackingAreaOptions.activeAlways
             | NSTrackingAreaOptions.inVisibleRect
    let area = NSTrackingArea.alloc().init(
        makeRect(0.0, 0.0, width, height), opts,
        tf as NSObject, nil
    )
    (tf as NSView).addTrackingArea(area as NSObject)

    ChangeHandler.register()
    let h = ChangeHandler.alloc().init()
    tf.setDelegate(h as NSObject)

    parent.addSubview(tf as NSView)

    let handle = (tf as NSView).handle
    let slot = new EditSlot(tf, h as NSObject)
    EDITS[handle] = slot

    // NSEvent local monitor for key events. The field editor
    // (the NSText shared by the window, not this NSTextField)
    // is the actual firstResponder once the field has focus, so
    // a `keyDown:` override on FocusTextField would never fire.
    // A process-local monitor sees keyDown/keyUp before they're
    // dispatched up the responder chain; filter by
    // `window.firstResponder == textfield.currentEditor` so only
    // keys actually aimed at this field surface as our Signal.
    let keyMask = NSEventMask.keyDown | NSEventMask.keyUp
    slot.keyMonitor = NSEvent.addLocalMonitorForEventsMatchingMask(
        keyMask,
        new ObjCBlock(fn(evtRaw: i64): i64 {
            let evt = NSEvent.__wrap_handle_unowned(evtRaw)
            let winObj = evt.window
            if winObj.handle != 0 as i64 {
                let win = NSWindow.__wrap_handle_unowned(winObj.handle)
                let resp = win.firstResponder
                let editor = slot.edit.currentEditor
                if editor.handle != 0 as i64 && resp.handle == editor.handle {
                    let flags = evt.modifierFlags as u64
                    let code = keyCodeToString(evt.keyCode)
                    let ck = modCtrl(flags)
                    let sk = modShift(flags)
                    let ak = modAlt(flags)
                    let mk = modMeta(flags)
                    if (evt.eventType as u64) == 10 as u64 {
                        slot.onKeyDown(code, ck, sk, ak, mk)
                    } else {
                        slot.onKeyUp(code, ck, sk, ak, mk)
                    }
                }
            }
            // Always forward — swallowing would block AppKit's
            // own keystroke handling (insert text into the field
            // editor, command keys, etc.).
            evtRaw
        })
    )

    handle
}

pub fn readEditText(handle: i64): string {
    match EDITS.get(handle) {
        some(slot) { ilstr(slot.edit.stringValue) }
        none       { "" }
    }
}

pub fn setOnEditChange(handle: i64, cb: fn(string)) {
    if let some(slot) = EDITS.get(handle) { slot.onChange = cb }
}
pub fn setOnEditFocus(handle: i64, cb: fn()) {
    if let some(slot) = EDITS.get(handle) { slot.onFocus = cb }
}
pub fn setOnEditBlur(handle: i64, cb: fn()) {
    if let some(slot) = EDITS.get(handle) { slot.onBlur = cb }
}
pub fn setOnEditMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = EDITS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnEditMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = EDITS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnEditMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = EDITS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnEditMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = EDITS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnEditContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = EDITS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnEditKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = EDITS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnEditKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = EDITS.get(handle) { slot.onKeyUp = cb }
}
pub fn setOnEditMouseWheel(
    handle: i64,
    cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = EDITS.get(handle) { slot.onMouseWheel = cb }
}

/// Internal: hand the field editor wrapped as NSText, or `nil`
/// when the field isn't focused (no editor allocated yet).
fn editorOf(slot: EditSlot): NSText {
    let raw = slot.edit.currentEditor
    if raw.handle == 0 as i64 {
        // `NSText.wrap(0)` returns a typed nil — callers compare
        // .handle before dereferencing.
        NSText.__wrap_handle_unowned(0 as i64)
    } else {
        NSText.__wrap_handle_unowned(raw.handle)
    }
}

pub fn selectAllEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        // `selectText:` makes the field first responder and
        // selects all in one step — the same path AppKit takes
        // when the user Tab-cycles into the field.
        slot.edit.selectText(nil)
    }
}

pub fn setEditSelection(handle: i64, start: i64, end: i64) {
    if let some(slot) = EDITS.get(handle) {
        // The field editor must exist to carry a selection.
        // Force focus first so AppKit hands one back.
        let editor0 = editorOf(slot)
        let editor = if editor0.handle == 0 as i64 {
            let win = (slot.edit as NSView).window
            let _ = win.makeFirstResponder(slot.edit as NSObject)
            editorOf(slot)
        } else { editor0 }
        if editor.handle != 0 as i64 {
            let s = if start < 0 as i64 { 0 as i64 } else { start }
            let e = if end < s { s } else { end }
            editor.setSelectedRange(
                NSRange { location: s as u64, length: (e - s) as u64 }
            )
        }
    }
}

pub fn getEditSelectionStart(handle: i64): i64 {
    if let some(slot) = EDITS.get(handle) {
        let editor = editorOf(slot)
        if editor.handle == 0 as i64 { return 0 as i64 }
        editor.selectedRange().location as i64
    } else {
        0 as i64
    }
}

pub fn getEditSelectionEnd(handle: i64): i64 {
    if let some(slot) = EDITS.get(handle) {
        let editor = editorOf(slot)
        if editor.handle == 0 as i64 { return 0 as i64 }
        let r = editor.selectedRange()
        (r.location + r.length) as i64
    } else {
        0 as i64
    }
}
pub fn focusEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        // Ask the parent window to install the text field as
        // first responder. AppKit then spins up the shared field
        // editor whose initial editing notification surfaces as
        // `onFocus` via `controlTextDidBeginEditing:`.
        let win = (slot.edit as NSView).window
        let _ = win.makeFirstResponder(slot.edit as NSObject)
    }
}
pub fn blurEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        // `currentEditor` is non-nil iff this NSTextField has the
        // field editor attached — i.e. it's the focus holder.
        // Match `HTMLElement.blur()` web semantics: only act when
        // we own focus, otherwise yanking it would clear whichever
        // sibling control actually has it.
        let editor = slot.edit.currentEditor
        if editor.handle != 0 as i64 {
            let win = (slot.edit as NSView).window
            let _ = win.makeFirstResponder(nil)
        }
    }
}
pub fn setEditText(handle: i64, text: string) {
    if let some(slot) = EDITS.get(handle) { slot.edit.stringValue = nsstr(text) }
}
pub fn setEditFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = EDITS.get(handle) {
        slot.edit.setFrame(makeRect(x, y, width, height))
    }
}
pub fn setEditEnabled(handle: i64, enabled: bool) {
    if let some(slot) = EDITS.get(handle) { slot.edit.setEnabled(enabled) }
}
pub fn setEditVisible(handle: i64, visible: bool) {
    if let some(slot) = EDITS.get(handle) { slot.edit.setHidden(!visible) }
}
pub fn setEditPlaceholder(handle: i64, text: string) {
    if let some(slot) = EDITS.get(handle) {
        slot.edit.setPlaceholderString(nsstr(text))
    }
}
pub fn setEditReadOnly(handle: i64, readOnly: bool) {
    if let some(slot) = EDITS.get(handle) { slot.edit.setEditable(!readOnly) }
}
pub fn setEditMaxLength(handle: i64, max: i32) {
    if let some(slot) = EDITS.get(handle) { slot.maxLength = max }
}
pub fn setEditTooltip(handle: i64, text: string) {
    if let some(slot) = EDITS.get(handle) {
        // Empty string → pass `nil` so AppKit clears the tooltip
        // outright rather than registering an empty bubble.
        if text == "" {
            (slot.edit as NSView).setToolTip(nil)
        } else {
            (slot.edit as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}

/// Forward an `editor` selector (copy: / paste: / undo: / …)
/// to the field editor. When the field isn't focused there's no
/// editor and these are silent no-ops.
fn editorAction(slot: EditSlot, sel: i64) {
    let editor = slot.edit.currentEditor
    if editor.handle == 0 as i64 { return }
    (editor as NSObject).performSelector(sel)
}

pub fn copyEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) { editorAction(slot, selectorPtr("copy:")) }
}
pub fn cutEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) { editorAction(slot, selectorPtr("cut:")) }
}
pub fn pasteEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) { editorAction(slot, selectorPtr("paste:")) }
}
pub fn undoEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        editorAction(slot, selectorPtr("undo:"))
    }
}
pub fn redoEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        editorAction(slot, selectorPtr("redo:"))
    }
}
pub fn scrollEditToCaret(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        let editor = editorOf(slot)
        if editor.handle == 0 as i64 { return }
        let r = editor.selectedRange()
        editor.scrollRangeToVisible(r)
    }
}
