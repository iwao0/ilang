/// Multi-column table — `NSTableView` with N `NSTableColumn`s,
/// view-based cells. Column identity is the column index encoded
/// as a string (`"0"`, `"1"`, ...); the data source reads it back
/// to look up the right cell from each row's string array.
///
/// `setDoubleAction:` is wired to a `TableActivate` handler so
/// double-click on a row fires `onActivate`.

use cocoa {
    NSObject, NSView, NSTableView, NSTableColumn, NSTableCellView,
    NSScrollView, NSTextField,
    NSNotification, NSEvent, NSString,
    makeRect, makeScrollView, nsstr, ilstr, nil, selectorPtr,
    classPtr, setObjectClass,
}
use core { resolveParentView, noOp1, noOpI32 }

class TableSlot {
    pub table:    NSTableView
    pub scroll:   NSScrollView
    pub columns:  NSTableColumn[]
    pub rows:     string[][]
    pub onSelect:   fn(i32)
    pub onActivate: fn(i32)
    pub onFocus:    fn()
    pub onBlur:     fn()
    pub dataSource:    NSObject
    pub delegate:      NSObject
    pub actionHandler: NSObject
    pub init(table: NSTableView, scroll: NSScrollView,
             ds: NSObject, dg: NSObject, ah: NSObject) {
        this.table   = table
        this.scroll  = scroll
        this.columns = []
        this.rows    = []
        this.onSelect   = noOpI32
        this.onActivate = noOpI32
        this.onFocus    = noOp1
        this.onBlur     = noOp1
        this.dataSource    = ds
        this.delegate      = dg
        this.actionHandler = ah
    }
}

/// Marker NSView subclass — needed because the cocoa binding
/// doesn't expose `NSView.alloc()` directly. We don't actually
/// use it for view-based mode here (NSTableCellView is the
/// returned type) but the class must be registered for the
/// @objc desugar to leave the module happy. Removed once we
/// stop creating any auxiliary cell containers.
class TableCellHost : NSView {
    pub _markerSelf(): NSObject { this as NSObject }
}

/// Keyed by `table.handle`.
let TABLES = new Map<i64, TableSlot>()
/// dataSource / delegate / actionHandler handle → owning table key.
let DS_TO_TABLE: Map<i64, i64> = new Map<i64, i64>()

class TableDataSource : NSObject {
    @objc("numberOfRowsInTableView:")
    pub numberOfRowsInTableView(t: NSTableView): i64 {
        if let some(tk) = DS_TO_TABLE.get(this.handle) {
            if let some(slot) = TABLES.get(tk) {
                return slot.rows.length
            }
        }
        0 as i64
    }

    @objc("tableView:objectValueForTableColumn:row:")
    pub tableViewObjectValueForTableColumn(
        t: NSTableView, column: NSTableColumn, row: i64
    ): NSObject {
        if let some(tk) = DS_TO_TABLE.get(this.handle) {
            if let some(slot) = TABLES.get(tk) {
                if row < 0 as i64 || row >= slot.rows.length { return nsstr("") as NSObject }
                let rd = slot.rows[row]
                // Identifier is the column index encoded as a
                // string. Iterate columns to find the matching
                // column — at most a handful in practice.
                let n = slot.columns.length
                let i: i64 = 0
                while i < n {
                    if (slot.columns[i] as NSObject).handle == (column as NSObject).handle {
                        if i < rd.length {
                            return nsstr(rd[i]) as NSObject
                        }
                        return nsstr("") as NSObject
                    }
                    i = i + 1
                }
            }
        }
        nsstr("") as NSObject
    }
}

class TableDelegate : NSObject {
    pub tableViewSelectionDidChange(n: NSNotification) {
        if let some(tk) = DS_TO_TABLE.get(this.handle) {
            if let some(slot) = TABLES.get(tk) {
                let cb = slot.onSelect
                cb(slot.table.selectedRow as i32)
            }
        }
    }

    /// `tableView:viewForTableColumn:row:` — view-based cell.
    @objc("tableView:viewForTableColumn:row:")
    pub tableViewViewForTableColumn(
        t: NSTableView, column: NSTableColumn, row: i64
    ): NSView {
        let ident = column.identifier
        let reused = t.makeViewWithIdentifier(ident, nil)
        let cellView: NSTableCellView = if reused.handle != 0 as i64 {
            NSTableCellView.__wrap_handle_unowned(reused.handle)
        } else {
            let cv = NSTableCellView.alloc().init(makeRect(0.0, 0.0, 120.0, 20.0))
            (cv as NSView).setIdentifier(ident)
            let tf = NSTextField.alloc().init(makeRect(2.0, 2.0, 116.0, 16.0))
            tf.setEditable(false)
            tf.setBordered(false)
            tf.setBezeled(false)
            tf.setDrawsBackground(false)
            tf.setSelectable(false)
            (cv as NSView).addSubview(tf as NSView)
            cv.textField = tf
            cv
        }
        // Bind the textField to this cell. The string lookup
        // mirrors the data-source path so we don't need to plumb
        // objectValue through.
        if let some(tk) = DS_TO_TABLE.get(this.handle) {
            if let some(slot) = TABLES.get(tk) {
                if row >= 0 as i64 && row < slot.rows.length {
                    let rd = slot.rows[row]
                    let n = slot.columns.length
                    let i: i64 = 0
                    while i < n {
                        if (slot.columns[i] as NSObject).handle == (column as NSObject).handle {
                            let s = if i < rd.length { rd[i] } else { "" }
                            cellView.textField.stringValue = nsstr(s)
                            i = n
                        }
                        i = i + 1
                    }
                }
            }
        }
        cellView as NSView
    }
}

class TableActionHandler : NSObject {
    @objc("tableDoubleClicked:")
    pub tableDoubleClicked(sender: NSObject) {
        if let some(tk) = DS_TO_TABLE.get(this.handle) {
            if let some(slot) = TABLES.get(tk) {
                let row = slot.table.clickedRow
                if row >= 0 as i64 {
                    let cb = slot.onActivate
                    cb(row as i32)
                }
            }
        }
    }
}

@extern(C) {

pub fn createNativeTable(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let _ = enabled
    let table = NSTableView.alloc().init(makeRect(0.0, 0.0, width, height))
    table.allowsMultipleSelection = false
    table.rowHeight = 20.0
    // Double-click handling.
    TableActionHandler.register()
    let ah = TableActionHandler.alloc().init()
    table.setTarget(ah as NSObject)
    table.doubleAction = selectorPtr("tableDoubleClicked:")

    TableDataSource.register()
    TableDelegate.register()
    TableCellHost.register()
    let ds = TableDataSource.alloc().init()
    let dg = TableDelegate.alloc().init()
    table.setDataSource(ds as NSObject)
    table.setDelegate(dg as NSObject)

    let scroll = makeScrollView(table as NSView, x, y, width, height)
    scroll.setHidden(!visible)
    parent.addSubview(scroll as NSView)

    let tableKey = (table as NSView).handle
    TABLES.set(tableKey,
        new TableSlot(table, scroll, ds as NSObject, dg as NSObject, ah as NSObject))
    DS_TO_TABLE.set((ds as NSObject).handle, tableKey)
    DS_TO_TABLE.set((dg as NSObject).handle, tableKey)
    DS_TO_TABLE.set((ah as NSObject).handle, tableKey)
    tableKey
}

pub fn tableAddColumn(tableKey: i64, title: string, width: f64) {
    if let some(slot) = TABLES.get(tableKey) {
        let idx = slot.columns.length
        let col = NSTableColumn.alloc().initWithIdentifier(nsstr(intToString(idx)))
        col.isEditable = false
        col.title = nsstr(title)
        col.width = width
        col.minWidth = 24.0
        slot.table.addTableColumn(col)
        slot.columns.push(col)
        slot.table.reloadData()
    }
}

/// Helper: minimal i64 → decimal string. ilang's standard
/// library wires int formatting through `nsstr` overloads that
/// require Foundation; we only need 0..N here for column ids.
fn intToString(n: i64): string {
    let digits: string[] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"]
    if n == 0 as i64 { return "0" }
    let neg = n < 0 as i64
    let acc: i64 = if neg { -n } else { n }
    let buf: string[] = []
    while acc > 0 as i64 {
        let d = acc % (10 as i64)
        buf.unshift(digits[d])
        acc = acc / (10 as i64)
    }
    let out = if neg { "-" } else { "" }
    let m = buf.length
    let i: i64 = 0
    while i < m {
        out = out + buf[i]
        i = i + 1
    }
    out
}

pub fn tableAddRow(tableKey: i64, cells: string[]) {
    if let some(slot) = TABLES.get(tableKey) {
        slot.rows.push(cells)
        slot.table.reloadData()
    }
}

pub fn clearTableRows(tableKey: i64) {
    if let some(slot) = TABLES.get(tableKey) {
        slot.rows = []
        slot.table.reloadData()
    }
}

pub fn readTableSelectedRow(tableKey: i64): i32 {
    match TABLES.get(tableKey) {
        some(slot) { slot.table.selectedRow as i32 }
        none       { -1 as i32 }
    }
}

pub fn setTableSelectedRow(tableKey: i64, row: i32) {
    if let some(slot) = TABLES.get(tableKey) {
        if row < 0 as i32 {
            slot.table.deselectAll(nil)
        }
        // Selecting a specific row needs an NSIndexSet, which
        // the cocoa bindings don't expose a single-row
        // constructor for today — same Round 1 gap as ListBox.
    }
}

pub fn setTableFrame(tableKey: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = TABLES.get(tableKey) {
        slot.scroll.setFrame(makeRect(x, y, width, height))
    }
}

pub fn setTableEnabled(tableKey: i64, enabled: bool) {
    let _ = tableKey
    let _ = enabled
    // NSTableView has no direct enabled toggle.
}

pub fn setTableVisible(tableKey: i64, visible: bool) {
    if let some(slot) = TABLES.get(tableKey) { slot.scroll.setHidden(!visible) }
}

pub fn setTableTooltip(tableKey: i64, text: string) {
    if let some(slot) = TABLES.get(tableKey) {
        if text == "" {
            (slot.scroll as NSView).setToolTip(nil)
        } else {
            (slot.scroll as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}

pub fn setOnTableSelect(tableKey: i64, cb: fn(i32)) {
    if let some(slot) = TABLES.get(tableKey) { slot.onSelect = cb }
}
pub fn setOnTableActivate(tableKey: i64, cb: fn(i32)) {
    if let some(slot) = TABLES.get(tableKey) { slot.onActivate = cb }
}
pub fn setOnTableFocus(tableKey: i64, cb: fn()) {
    if let some(slot) = TABLES.get(tableKey) { slot.onFocus = cb }
}
pub fn setOnTableBlur(tableKey: i64, cb: fn()) {
    if let some(slot) = TABLES.get(tableKey) { slot.onBlur = cb }
}
pub fn setOnTableMouseEnter(tableKey: i64, cb: fn(f64, f64)) {
    let _ = tableKey; let _ = cb
}
pub fn setOnTableMouseLeave(tableKey: i64, cb: fn(f64, f64)) {
    let _ = tableKey; let _ = cb
}
pub fn setOnTableMouseDown(
    tableKey: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = tableKey; let _ = cb }
pub fn setOnTableMouseUp(
    tableKey: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = tableKey; let _ = cb }
pub fn setOnTableContextMenu(
    tableKey: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = tableKey; let _ = cb }
pub fn setOnTableMouseWheel(
    tableKey: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) { let _ = tableKey; let _ = cb }
pub fn setOnTableKeyDown(tableKey: i64, cb: fn(string, bool, bool, bool, bool)) {
    let _ = tableKey; let _ = cb
}
pub fn setOnTableKeyUp(tableKey: i64, cb: fn(string, bool, bool, bool, bool)) {
    let _ = tableKey; let _ = cb
}

pub fn focusTable(tableKey: i64) {
    if let some(slot) = TABLES.get(tableKey) {
        let win = (slot.table as NSView).window
        let _ = win.makeFirstResponder(slot.table as NSObject)
    }
}
pub fn blurTable(tableKey: i64) {
    if let some(slot) = TABLES.get(tableKey) {
        let win = (slot.table as NSView).window
        if win.firstResponder.handle == (slot.table as NSView).handle {
            let _ = win.makeFirstResponder(nil)
        }
    }
}

} // @extern(C)
