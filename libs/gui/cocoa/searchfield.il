/// SearchField. `NSSearchField` with the search action wired
/// to a single handler that splits "search" vs "clear" by
/// inspecting `stringValue`. `controlTextDidChange:` on the
/// delegate fires `onChange` for every keystroke.

use cocoa {
    NSObject, NSSearchField, NSTextField, NSView,
    NSEvent, NSNotification,
    NSTrackingArea, NSTrackingAreaOptions,
    nsstr, ilstr, nil, makeRect, selectorPtr,
    classPtr, setObjectClass,
}
use core {
    WINDOWS, resolveParentView, noOpText, noOp1, noOpKey,
    keyCodeToString, modCtrl, modShift, modAlt, modMeta,
}

class SearchFieldSlot {
    pub field:    NSSearchField
    pub onChange: fn(string)
    pub onSearch: fn(string)
    pub onClear:  fn()
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub onKeyDown: fn(string, bool, bool, bool, bool)
    pub onKeyUp:   fn(string, bool, bool, bool, bool)
    pub handler:  NSObject
    pub delegate: NSObject
    /// `true` while the field is already known to be empty —
    /// blocks the duplicate `onClear` AppKit fires after a
    /// cancel-button click. Reset to `false` on any non-empty
    /// content change.
    pub lastWasClear: bool
    pub init(field: NSSearchField, handler: NSObject, delegate: NSObject) {
        this.field = field
        this.onChange = noOpText
        this.onSearch = noOpText
        this.onClear  = noOp1
        this.onFocus  = noOp1
        this.onBlur   = noOp1
        this.onKeyDown = noOpKey
        this.onKeyUp   = noOpKey
        this.handler = handler
        this.delegate = delegate
        this.lastWasClear = false
    }
}

let SEARCHFIELDS = new Map<i64, SearchFieldSlot>()

/// NSSearchField subclass — captures responder transitions and
/// key events. Mirrors the FocusTextField pattern from edit.il.
class FocusSearchField : NSSearchField {
    pub becomeFirstResponder(): bool {
        if let some(slot) = SEARCHFIELDS.get(this.handle) {
            let cb = slot.onFocus
            cb()
        }
        super.becomeFirstResponder()
    }
    pub resignFirstResponder(): bool {
        if let some(slot) = SEARCHFIELDS.get(this.handle) {
            let cb = slot.onBlur
            cb()
        }
        super.resignFirstResponder()
    }
    pub keyDown(event: NSEvent) {
        if let some(slot) = SEARCHFIELDS.get(this.handle) {
            let flags = event.modifierFlags as u64
            let cb = slot.onKeyDown
            cb(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
    pub keyUp(event: NSEvent) {
        if let some(slot) = SEARCHFIELDS.get(this.handle) {
            let flags = event.modifierFlags as u64
            let cb = slot.onKeyUp
            cb(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
}

/// Target/action handler — fires only when NSSearchField commits
/// (Return) or the cancel button is clicked. Empty `stringValue`
/// after the cancel-button press means "clear"; everything else
/// is a real search.
class SearchActionHandler : NSObject {
    pub searched(sender: NSObject) {
        if let some(slot) = SEARCHFIELDS.get(sender.handle) {
            let v = ilstr(slot.field.stringValue)
            if v == "" {
                // AppKit fires the action twice on cancel-button
                // clicks (once for the button, once for the
                // search committing the now-empty value). Collapse
                // the second one.
                if slot.lastWasClear { return }
                slot.lastWasClear = true
                let cb = slot.onClear
                cb()
            } else {
                slot.lastWasClear = false
                let cb = slot.onSearch
                cb(v)
            }
        }
    }
}

/// `controlTextDidChange:` arrives in here on every keystroke;
/// we forward the new stringValue as `onChange`.
class SearchChangeDelegate : NSObject {
    pub controlTextDidChange(n: NSNotification) {
        if let some(slot) = SEARCHFIELDS.get(n.notificationObject.handle) {
            let v = ilstr(slot.field.stringValue)
            if v != "" { slot.lastWasClear = false }
            let cb = slot.onChange
            cb(v)
        }
    }
}

pub fn createNativeSearchField(
    windowHandle: i64,
    text: string, placeholder: string,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let sf = NSSearchField.alloc().init(makeRect(x, y, width, height))
    sf.stringValue = nsstr(text)
    sf.setPlaceholderString(nsstr(placeholder))
    sf.setEnabled(enabled)
    (sf as NSView).setHidden(!visible)
    // Action fires on Return + cancel-button click only; we read
    // stringValue inside the handler to tell them apart.
    sf.setSendsWholeSearchString(true)
    sf.setSendsSearchStringImmediately(false)

    SearchActionHandler.register()
    let h = SearchActionHandler.alloc().init()
    // NSSearchField inherits target/action and setDelegate: from
    // NSTextField (which inherits target/action from NSControl);
    // call them through the explicit upcast so ilang's method
    // lookup binds to the parent declaration.
    (sf as NSTextField).setTarget(h as NSObject)
    (sf as NSTextField).setAction(selectorPtr("searched:"))

    SearchChangeDelegate.register()
    let d = SearchChangeDelegate.alloc().init()
    (sf as NSTextField).setDelegate(d as NSObject)

    FocusSearchField.register()
    let focusCls = classPtr("gui_impl.FocusSearchField")
    if focusCls != 0 as i64 {
        let _ = setObjectClass((sf as NSView).handle, focusCls)
    }

    parent.addSubview(sf as NSView)

    let handle = sf.handle
    SEARCHFIELDS[handle] = new SearchFieldSlot(sf, h as NSObject, d as NSObject)
    handle
}

pub fn readSearchFieldText(handle: i64): string {
    match SEARCHFIELDS.get(handle) {
        some(slot) { ilstr(slot.field.stringValue) }
        none       { "" }
    }
}
pub fn setSearchFieldText(handle: i64, text: string) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        slot.field.stringValue = nsstr(text)
    }
}
pub fn setSearchFieldPlaceholder(handle: i64, text: string) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        slot.field.setPlaceholderString(nsstr(text))
    }
}
pub fn setSearchFieldFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        (slot.field as NSView).setFrame(makeRect(x, y, width, height))
    }
}
pub fn setSearchFieldEnabled(handle: i64, enabled: bool) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.field.setEnabled(enabled) }
}
pub fn setSearchFieldVisible(handle: i64, visible: bool) {
    if let some(slot) = SEARCHFIELDS.get(handle) { (slot.field as NSView).setHidden(!visible) }
}
pub fn setSearchFieldTooltip(handle: i64, text: string) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        if text == "" {
            (slot.field as NSView).setToolTip(nil)
        } else {
            (slot.field as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}
pub fn setOnSearchFieldChange(handle: i64, cb: fn(string)) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onChange = cb }
}
pub fn setOnSearchFieldSearch(handle: i64, cb: fn(string)) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onSearch = cb }
}
pub fn setOnSearchFieldClear(handle: i64, cb: fn()) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onClear = cb }
}
pub fn setOnSearchFieldFocus(handle: i64, cb: fn()) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onFocus = cb }
}
pub fn setOnSearchFieldBlur(handle: i64, cb: fn()) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onBlur = cb }
}
pub fn setOnSearchFieldKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnSearchFieldKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onKeyUp = cb }
}
pub fn focusSearchField(handle: i64) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        let win = (slot.field as NSView).window
        let _ = win.makeFirstResponder(slot.field as NSObject)
    }
}
pub fn blurSearchField(handle: i64) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        let win = (slot.field as NSView).window
        if win.firstResponder.handle == slot.field.handle {
            let _ = win.makeFirstResponder(nil)
        }
    }
}
