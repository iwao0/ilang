// NSTableView + NSOutlineView family — multi-column / tree
// list views plus their delegate / data-source protocols and
// NSTableColumn.

use foundation { NSObject, NSString, NSArray, NSIndexSet, NSRect,
                 NSSize, NSNotification }
use app { NSView }

// NSTableViewSelectionHighlightStyle — visual treatment of the
// selected row.
pub enum NSTableViewSelectionHighlightStyle: i64 {
    none        = -1
    regular     = 0
    sourceList  = 1
}

// NSTableViewColumnAutoresizingStyle — how column widths react
// to the table being resized.
pub enum NSTableViewColumnAutoresizingStyle: u64 {
    none                          = 0
    uniform                       = 1
    sequential                    = 2
    reverseSequential             = 3
    lastColumnOnly                = 4
    firstColumnOnly               = 5
}

// NSTableViewGridLineStyle — bit flags for the grid lines drawn
// between rows / columns. Combine with `|`.
@flags
pub enum NSTableViewGridLineStyle: u64 {
    none                = 0
    solidVerticalLine   = 1
    solidHorizontalLine = 2
    dashedHorizontalLine = 8
}

@extern(ObjC, "/System/Library/Frameworks/AppKit.framework/AppKit") {
    /// `NSTableColumn` — one column in an NSTableView. Has an
    /// identifier string (used in delegate / data-source
    /// callbacks to disambiguate columns) plus header / width
    /// / min-width / max-width settings.
    @objc pub class NSTableColumn : NSObject {
        @objc("alloc")
        pub static alloc(): NSTableColumn
        @objc("initWithIdentifier:")
        pub initWithIdentifier(id: NSString): NSTableColumn

        @objc("identifier")
        pub get identifier(): NSString
        @objc("setIdentifier:")
        pub set identifier(s: NSString)
        @objc("title")
        pub get title(): NSString
        @objc("setTitle:")
        pub set title(s: NSString)
        @objc("width")
        pub get width(): f64
        @objc("setWidth:")
        pub set width(w: f64)
        @objc("minWidth")
        pub get minWidth(): f64
        @objc("setMinWidth:")
        pub set minWidth(w: f64)
        @objc("maxWidth")
        pub get maxWidth(): f64
        @objc("setMaxWidth:")
        pub set maxWidth(w: f64)
        @objc("isEditable")
        pub get isEditable(): bool
        @objc("setEditable:")
        pub set isEditable(flag: bool)
        @objc("isHidden")
        pub get isHidden(): bool
        @objc("setHidden:")
        pub set isHidden(flag: bool)
    }

    /// `NSTableView` — multi-column row list. Build the table,
    /// add columns via `addTableColumn`, set a data source (must
    /// implement `NSTableViewDataSource`) and optionally a
    /// delegate (`NSTableViewDelegate`), then call `reloadData`
    /// whenever the underlying model changes.
    ///
    /// Typical embedding is inside an `NSScrollView`
    /// (`makeScrollView(table, …)` from `layout.il`).
    @objc pub class NSTableView : NSView {
        @objc("alloc")
        pub static alloc(): NSTableView
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSTableView

        @objc("reloadData")
        pub reloadData()
        @objc("reloadDataForRowIndexes:columnIndexes:")
        pub reloadRows(rows: NSIndexSet, columnIndexes: NSIndexSet)
        @objc("noteNumberOfRowsChanged")
        pub noteNumberOfRowsChanged()

        @objc("addTableColumn:")
        pub addTableColumn(col: NSTableColumn)
        @objc("removeTableColumn:")
        pub removeTableColumn(col: NSTableColumn)
        @objc("tableColumns")
        pub get tableColumns(): NSArray
        @objc("numberOfColumns")
        pub get numberOfColumns(): i64
        @objc("numberOfRows")
        pub get numberOfRows(): i64

        @objc("rowHeight")
        pub get rowHeight(): f64
        @objc("setRowHeight:")
        pub set rowHeight(h: f64)
        @objc("intercellSpacing")
        pub get intercellSpacing(): NSSize
        @objc("setIntercellSpacing:")
        pub set intercellSpacing(s: NSSize)

        @objc("selectedRow")
        pub get selectedRow(): i64
        @objc("selectedColumn")
        pub get selectedColumn(): i64
        @objc("selectedRowIndexes")
        pub get selectedRowIndexes(): NSIndexSet
        @objc("selectRowIndexes:byExtendingSelection:")
        pub selectRowIndexes(indexes: NSIndexSet, byExtendingSelection: bool)
        @objc("deselectRow:")
        pub deselectRow(row: i64)
        @objc("deselectAll:")
        pub deselectAll(sender: NSObject)
        @objc("allowsEmptySelection")
        pub get allowsEmptySelection(): bool
        @objc("setAllowsEmptySelection:")
        pub set allowsEmptySelection(flag: bool)
        @objc("allowsMultipleSelection")
        pub get allowsMultipleSelection(): bool
        @objc("setAllowsMultipleSelection:")
        pub set allowsMultipleSelection(flag: bool)

        @objc("usesAlternatingRowBackgroundColors")
        pub get usesAlternatingRowBackgroundColors(): bool
        @objc("setUsesAlternatingRowBackgroundColors:")
        pub set usesAlternatingRowBackgroundColors(flag: bool)

        @objc("gridStyleMask")
        pub get gridStyleMask(): NSTableViewGridLineStyle
        @objc("setGridStyleMask:")
        pub set gridStyleMask(m: NSTableViewGridLineStyle)

        @objc("selectionHighlightStyle")
        pub get selectionHighlightStyle(): NSTableViewSelectionHighlightStyle
        @objc("setSelectionHighlightStyle:")
        pub set selectionHighlightStyle(s: NSTableViewSelectionHighlightStyle)

        @objc("columnAutoresizingStyle")
        pub get columnAutoresizingStyle(): NSTableViewColumnAutoresizingStyle
        @objc("setColumnAutoresizingStyle:")
        pub set columnAutoresizingStyle(s: NSTableViewColumnAutoresizingStyle)

        @objc("headerView")
        pub get headerView(): NSView
        @objc("setHeaderView:")
        pub set headerView(v: NSView)
        @objc("cornerView")
        pub get cornerView(): NSView
        @objc("setCornerView:")
        pub set cornerView(v: NSView)

        @objc("clickedRow")
        pub get clickedRow(): i64
        @objc("clickedColumn")
        pub get clickedColumn(): i64
        @objc("doubleAction")
        pub get doubleAction(): i64
        @objc("setDoubleAction:")
        pub set doubleAction(sel: i64)

        @objc("scrollRowToVisible:")
        pub scrollRowToVisible(row: i64)
        @objc("scrollColumnToVisible:")
        pub scrollColumnToVisible(col: i64)

        @objc("setDelegate:")
        pub setDelegate(d: NSTableViewDelegate)
        @objc("setDelegate:")
        pub setDelegate(d: NSObject)
        @objc("setDataSource:")
        pub setDataSource(d: NSTableViewDataSource)
        @objc("setDataSource:")
        pub setDataSource(d: NSObject)
    }

    /// `NSTableViewDataSource` — required for any NSTableView
    /// driven by a model object. The runtime calls
    /// `numberOfRowsInTableView:` to learn how many rows to
    /// draw, and
    /// `tableView:objectValueForTableColumn:row:` for each
    /// (column, row) cell. The cell value is an `id`: an
    /// `NSString` for plain text cells, an `NSNumber` for
    /// number / bool, etc.
    @objc pub interface NSTableViewDataSource {
        @optional @objc("numberOfRowsInTableView:")
        numberOfRowsInTableView(tableView: NSTableView): i64
        @optional @objc("tableView:objectValueForTableColumn:row:")
        tableViewObjectValueForTableColumn(
            tableView: NSTableView, objectValueForTableColumn: NSTableColumn, row: i64
        ): NSObject
        @optional @objc("tableView:setObjectValue:forTableColumn:row:")
        tableViewSetObjectValue(
            tableView: NSTableView, setObjectValue: NSObject,
            forTableColumn: NSTableColumn, row: i64
        )
    }

    /// `NSTableViewDelegate` — selection / sizing / cell-view
    /// callbacks. Pair with a data source for a complete table.
    /// Implementing `tableView:viewForTableColumn:row:` (the
    /// view-based form) lets you return any NSView for the
    /// cell; the older cell-based form
    /// (`tableView:willDisplayCell:forTableColumn:row:`) is
    /// still supported for legacy `NSCell`-shaped tables.
    @objc pub interface NSTableViewDelegate {
        @optional @objc("tableView:viewForTableColumn:row:")
        tableViewViewForTableColumn(
            tableView: NSTableView, viewForTableColumn: NSTableColumn, row: i64
        ): NSView

        @optional @objc("tableView:heightOfRow:")
        tableViewHeightOfRow(tableView: NSTableView, heightOfRow: i64): f64
        @optional @objc("tableView:shouldSelectRow:")
        tableViewShouldSelectRow(tableView: NSTableView, shouldSelectRow: i64): bool
        @optional @objc("tableView:shouldSelectTableColumn:")
        tableViewShouldSelectTableColumn(
            tableView: NSTableView, shouldSelectTableColumn: NSTableColumn
        ): bool

        @optional tableViewSelectionDidChange(notification: NSNotification)
        @optional tableViewSelectionIsChanging(notification: NSNotification)
        @optional tableViewColumnDidResize(notification: NSNotification)
        @optional tableViewColumnDidMove(notification: NSNotification)
    }

    /// `NSOutlineView` — tree-shaped variant of NSTableView.
    /// Items in the tree are arbitrary `id`s (your model
    /// types); the outline view asks the data source for each
    /// item's children, expansion state, and per-column cell
    /// values. Disclosure triangles appear in the leftmost
    /// (outline) column.
    @objc pub class NSOutlineView : NSTableView {
        @objc("alloc")
        pub static alloc(): NSOutlineView
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSOutlineView

        @objc("expandItem:")
        pub expandItem(item: NSObject)
        @objc("expandItem:expandChildren:")
        pub expandItemRecursive(item: NSObject, expandChildren: bool)
        @objc("collapseItem:")
        pub collapseItem(item: NSObject)
        @objc("collapseItem:collapseChildren:")
        pub collapseItemRecursive(item: NSObject, collapseChildren: bool)
        @objc("isItemExpanded:")
        pub isItemExpanded(item: NSObject): bool
        @objc("rowForItem:")
        pub rowForItem(item: NSObject): i64
        @objc("itemAtRow:")
        pub itemAtRow(row: i64): NSObject
        @objc("levelForItem:")
        pub levelForItem(item: NSObject): i64
        @objc("levelForRow:")
        pub levelForRow(row: i64): i64
        @objc("parentForItem:")
        pub parentForItem(item: NSObject): NSObject
        @objc("reloadItem:")
        pub reloadItem(item: NSObject)
        @objc("reloadItem:reloadChildren:")
        pub reloadItemRecursive(item: NSObject, reloadChildren: bool)

        @objc("indentationPerLevel")
        pub get indentationPerLevel(): f64
        @objc("setIndentationPerLevel:")
        pub set indentationPerLevel(v: f64)
        @objc("outlineTableColumn")
        pub get outlineTableColumn(): NSTableColumn
        @objc("setOutlineTableColumn:")
        pub set outlineTableColumn(c: NSTableColumn)

        // setDelegate / setDataSource on the parent already accept
        // NSObject; for typed protocols specific to NSOutlineView,
        // pass the implementer through `setDelegate(d as NSObject)`
        // — the runtime still dispatches the protocol's selectors
        // (NSOutlineViewDelegate / NSOutlineViewDataSource above)
        // via respondsToSelector:.
    }

    /// `NSOutlineViewDataSource` — model interface for an
    /// outline view. `outlineView:child:ofItem:` returns the
    /// Nth child of an item (pass `nil` for the root), and
    /// `outlineView:numberOfChildrenOfItem:` returns how many
    /// children that item has.
    @objc pub interface NSOutlineViewDataSource {
        @optional @objc("outlineView:child:ofItem:")
        outlineViewChildOfItem(
            outlineView: NSOutlineView, child: i64, ofItem: NSObject
        ): NSObject
        @optional @objc("outlineView:isItemExpandable:")
        outlineViewIsItemExpandable(
            outlineView: NSOutlineView, isItemExpandable: NSObject
        ): bool
        @optional @objc("outlineView:numberOfChildrenOfItem:")
        outlineViewNumberOfChildrenOfItem(
            outlineView: NSOutlineView, numberOfChildrenOfItem: NSObject
        ): i64
        @optional @objc("outlineView:objectValueForTableColumn:byItem:")
        outlineViewObjectValueForTableColumn(
            outlineView: NSOutlineView,
            objectValueForTableColumn: NSTableColumn,
            byItem: NSObject
        ): NSObject
    }

    /// `NSOutlineViewDelegate` — selection / sizing / view
    /// callbacks for an NSOutlineView. Same shape as
    /// `NSTableViewDelegate` but the row identity is an
    /// arbitrary `id` (the item) rather than an integer row.
    @objc pub interface NSOutlineViewDelegate {
        @optional @objc("outlineView:viewForTableColumn:item:")
        outlineViewViewForTableColumn(
            outlineView: NSOutlineView,
            viewForTableColumn: NSTableColumn, item: NSObject
        ): NSView
        @optional @objc("outlineView:heightOfRowByItem:")
        outlineViewHeightOfRowByItem(
            outlineView: NSOutlineView, heightOfRowByItem: NSObject
        ): f64
        @optional @objc("outlineView:shouldSelectItem:")
        outlineViewShouldSelectItem(
            outlineView: NSOutlineView, shouldSelectItem: NSObject
        ): bool
        @optional outlineViewSelectionDidChange(notification: NSNotification)
    }
}
