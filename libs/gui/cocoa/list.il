/// Scrollable single-column list. NSTableView (one anonymous
/// column, no header) inside an NSScrollView. Items live on the
/// ilang side (`ListSlot.items`) and are served to the table via
/// the `ListDataSource` callbacks; selection changes hop through
/// `ListDelegate.tableViewSelectionDidChange:`.

use cocoa {
    NSObject, NSView, NSTableView, NSTableColumn, NSScrollView,
    NSNotification, NSEvent,
    NSTrackingArea, NSTrackingAreaOptions,
    nsstr, makeRect, makeScrollView, nil,
    classPtr, setObjectClass,
}
use core {
    WINDOWS, noOp1, noOpI32,
    noOpEnterLeave, noOpClickMod, noOpWheel,
    modCtrl, modShift, modAlt, modMeta,
}

class ListSlot {
    pub table:      NSTableView
    pub scroll:     NSScrollView
    pub items:      string[]
    pub onSelect:   fn(i32)
    pub onFocus:    fn()
    pub onBlur:     fn()
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub dataSource: NSObject   // ListDataSource instance keep-alive
    pub delegate:   NSObject   // ListDelegate instance keep-alive
    pub init(table: NSTableView, scroll: NSScrollView,
             dataSource: NSObject, delegate: NSObject) {
        this.table      = table
        this.scroll     = scroll
        this.items      = []
        this.onSelect   = noOpI32
        this.onFocus    = noOp1
        this.onBlur     = noOp1
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseDown  = noOpClickMod
        this.onMouseUp    = noOpClickMod
        this.onContextMenu = noOpClickMod
        this.onMouseWheel = noOpWheel
        this.dataSource = dataSource
        this.delegate   = delegate
    }
}

/// NSTableView subclass — isa-swapped so AppKit's first-responder
/// transitions surface as onFocus / onBlur. NSTableView accepts
/// first responder by default, so no acceptsFirstResponder override.
class FocusList : NSTableView {
    pub becomeFirstResponder(): bool {
        if let some(slot) = LISTS.get(this.handle) {
            let cb = slot.onFocus
            cb()
        }
        true
    }
    pub resignFirstResponder(): bool {
        if let some(slot) = LISTS.get(this.handle) {
            let cb = slot.onBlur
            cb()
        }
        true
    }
    pub override mouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
        let y = (this as NSView).frame.size.height - p.y
        let flags = e.modifierFlags as u64
        if let some(slot) = LISTS.get(this.handle) {
            let cb = slot.onMouseDown
            cb(p.x, y, 0,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.mouseDown(event)
    }
    pub override mouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = LISTS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            let cb = slot.onMouseUp
            cb(p.x, y, 0,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.mouseUp(event)
    }
    pub override rightMouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = LISTS.get(this.handle) {
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
        if let some(slot) = LISTS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            let cb = slot.onMouseUp
            cb(p.x, y, 2,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.rightMouseUp(event)
    }
    pub override otherMouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = LISTS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            let cb = slot.onMouseDown
            cb(p.x, y, 1,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.otherMouseDown(event)
    }
    pub override otherMouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = LISTS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = e.modifierFlags as u64
            let cb = slot.onMouseUp
            cb(p.x, y, 1,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
        super.otherMouseUp(event)
    }
    pub override mouseEntered(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = LISTS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let cb = slot.onMouseEnter
            cb(p.x, y)
        }
    }
    pub override mouseExited(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        if let some(slot) = LISTS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(e.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let cb = slot.onMouseLeave
            cb(p.x, y)
        }
    }
    pub scrollWheel(event: NSEvent) {
        if let some(slot) = LISTS.get(this.handle) {
            let p = (this as NSView).convertPointFromView(event.locationInWindow, nil)
            let y = (this as NSView).frame.size.height - p.y
            let flags = event.modifierFlags as u64
            let cb = slot.onMouseWheel
            cb(event.deltaX, event.deltaY, p.x, y,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
}

/// Keyed by NSTableView's raw ObjC pointer (`table.handle`),
/// matching the public-side handle gui-core stores.
let LISTS = new Map<i64, ListSlot>()
/// Reverse map: dataSource handle → list handle, so
/// `ListDataSource`'s callbacks can find their list without a
/// scan. (`this` inside the data-source method is the
/// ListDataSource instance, not the table.)
let DS_TO_LIST = new Map<i64, i64>()

class ListDataSource : NSObject {
    pub numberOfRowsInTableView(t: NSTableView): i64 {
        if let some(lh) = DS_TO_LIST.get(this.handle) {
            if let some(slot) = LISTS.get(lh) {
                return slot.items.length
            }
        }
        0
    }

    /// `tableView:objectValueForTableColumn:row:` — the real
    /// protocol selector. Without the explicit @objc override,
    /// auto-lift would generate `tableViewObjectValueForTableColumn:::`
    /// and NSTableView would silently never call us, leaving
    /// every row blank.
    @objc("tableView:objectValueForTableColumn:row:")
    pub tableViewObjectValueForTableColumn(
        t: NSTableView, column: NSTableColumn, row: i64
    ): NSObject {
        if let some(lh) = DS_TO_LIST.get(this.handle) {
            if let some(slot) = LISTS.get(lh) {
                if row >= 0 && row < slot.items.length {
                    return nsstr(slot.items[row]) as NSObject
                }
            }
        }
        nsstr("") as NSObject
    }
}

class ListDelegate : NSObject {
    pub tableViewSelectionDidChange(n: NSNotification) {
        if let some(slot) = LISTS.get(n.notificationObject.handle) {
            let cb = slot.onSelect
            cb(slot.table.selectedRow as i32)
        }
    }
}

pub fn createNativeList(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let _ = enabled   // NSTableView selection isn't gated by an
                      // enabled flag in the same way; surfaced as
                      // no-op so the gui-core call site stays
                      // platform-uniform.
    let table = NSTableView.alloc().init(makeRect(0.0, 0.0, width, height))
    // Single anonymous column — width matches the scroll-view
    // contents so the column spans the visible area.
    let col = NSTableColumn.alloc().initWithIdentifier(nsstr("v"))
    // NSTableColumn defaults to editable=YES. With our cell-based
    // rendering that draws a plain string, Tab on a focused list
    // would otherwise enter cell-edit mode, then exit into a
    // half-broken visual state where focus never advances. Mark
    // the column non-editable so Tab follows the responder chain.
    col.isEditable = false
    table.addTableColumn(col)
    table.setHeaderView(nil)
    table.allowsMultipleSelection = false

    ListDataSource.register()
    let ds = ListDataSource.alloc().init()
    table.setDataSource(ds as NSObject)

    ListDelegate.register()
    let dg = ListDelegate.alloc().init()
    table.setDelegate(dg as NSObject)

    FocusList.register()
    let focusCls = classPtr("gui_impl.FocusList")
    if focusCls != 0 as i64 {
        let _ = setObjectClass((table as NSView).handle, focusCls)
    }
    let opts = NSTrackingAreaOptions.mouseEnteredAndExited
             | NSTrackingAreaOptions.activeAlways
             | NSTrackingAreaOptions.inVisibleRect
    let area = NSTrackingArea.alloc().init(
        makeRect(0.0, 0.0, width, height), opts,
        table as NSObject, nil
    )
    (table as NSView).addTrackingArea(area as NSObject)

    let scroll = makeScrollView(table as NSView, x, y, width, height)
    scroll.setHidden(!visible)
    winSlot.win.contentView.addSubview(scroll as NSView)

    let handle = (table as NSView).handle
    LISTS[handle] = new ListSlot(table, scroll, ds as NSObject, dg as NSObject)
    DS_TO_LIST[(ds as NSObject).handle] = handle
    handle
}

pub fn setOnListSelect(handle: i64, cb: fn(i32)) {
    if let some(slot) = LISTS.get(handle) { slot.onSelect = cb }
}
pub fn addListItem(handle: i64, item: string) {
    if let some(slot) = LISTS.get(handle) {
        slot.items.push(item)
        slot.table.reloadData()
    }
}
pub fn clearListItems(handle: i64) {
    if let some(slot) = LISTS.get(handle) {
        slot.items = []
        slot.table.reloadData()
    }
}
pub fn setListSelectedIndex(handle: i64, index: i32) {
    if let some(slot) = LISTS.get(handle) {
        if index < 0 {
            slot.table.deselectAll(nil)
        } else {
            // NSTableView wants an NSIndexSet; the cocoa bindings
            // don't expose a single-row constructor today, so we
            // fall back to selecting nothing and letting the user
            // re-pick.
        }
    }
}
pub fn readListSelectedIndex(handle: i64): i32 {
    match LISTS.get(handle) {
        some(slot) { slot.table.selectedRow as i32 }
        none       { -1 as i32 }
    }
}
pub fn setListFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = LISTS.get(handle) {
        slot.scroll.setFrame(makeRect(x, y, width, height))
    }
}
pub fn setListEnabled(handle: i64, enabled: bool) {
    // No direct NSTableView enabled toggle; surface as no-op.
}
pub fn setListVisible(handle: i64, visible: bool) {
    if let some(slot) = LISTS.get(handle) { slot.scroll.setHidden(!visible) }
}
pub fn setOnListFocus(handle: i64, cb: fn()) {
    if let some(slot) = LISTS.get(handle) { slot.onFocus = cb }
}
pub fn setOnListBlur(handle: i64, cb: fn()) {
    if let some(slot) = LISTS.get(handle) { slot.onBlur = cb }
}
pub fn focusList(handle: i64) {
    if let some(slot) = LISTS.get(handle) {
        let win = (slot.table as NSView).window
        let _ = win.makeFirstResponder(slot.table as NSObject)
    }
}
pub fn blurList(handle: i64) {
    if let some(slot) = LISTS.get(handle) {
        let win = (slot.table as NSView).window
        if win.firstResponder.handle == (slot.table as NSView).handle {
            let _ = win.makeFirstResponder(nil)
        }
    }
}
pub fn setOnListMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = LISTS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnListMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = LISTS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnListMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = LISTS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnListMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = LISTS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnListContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = LISTS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnListMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = LISTS.get(handle) { slot.onMouseWheel = cb }
}
pub fn setListTooltip(handle: i64, text: string) {
    if let some(slot) = LISTS.get(handle) {
        // Hang the tooltip off the scroll view — the table view
        // sits inside the scroller, so the hover hit-test is on
        // the scroll's frame anyway.
        if text == "" {
            (slot.scroll as NSView).setToolTip(nil)
        } else {
            (slot.scroll as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}
