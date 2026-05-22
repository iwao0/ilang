/// Scrollable single-column list. NSTableView (one anonymous
/// column, no header) inside an NSScrollView. Items live on the
/// ilang side (`ListSlot.items`) and are served to the table via
/// the `ListDataSource` callbacks; selection changes hop through
/// `ListDelegate.tableViewSelectionDidChange:`.

use cocoa {
    NSObject, NSView, NSTableView, NSTableColumn, NSScrollView,
    NSNotification, nsstr, makeRect, makeScrollView, nil,
}
use core { WINDOWS, noOpI32 }

class ListSlot {
    pub table:      NSTableView
    pub scroll:     NSScrollView
    pub items:      string[]
    pub onSelect:   fn(i32)
    pub dataSource: NSObject   // ListDataSource instance keep-alive
    pub delegate:   NSObject   // ListDelegate instance keep-alive
    pub init(table: NSTableView, scroll: NSScrollView,
             dataSource: NSObject, delegate: NSObject) {
        this.table      = table
        this.scroll     = scroll
        this.items      = []
        this.onSelect   = noOpI32
        this.dataSource = dataSource
        this.delegate   = delegate
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
        let _ = t
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
        let _ = t
        let _ = column
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
    table.addTableColumn(col)
    table.setHeaderView(nil)
    table.allowsMultipleSelection = false

    ListDataSource.register()
    let ds = ListDataSource.alloc().init()
    table.setDataSource(ds as NSObject)

    ListDelegate.register()
    let dg = ListDelegate.alloc().init()
    table.setDelegate(dg as NSObject)

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
            let _ = index
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
    let _ = handle
    let _ = enabled
}
pub fn setListVisible(handle: i64, visible: bool) {
    if let some(slot) = LISTS.get(handle) { slot.scroll.setHidden(!visible) }
}
