/// Multi-column table — `GtkColumnView` over a
/// `GListStore<GtkStringObject>` (one item per row), with one
/// `GtkSignalListItemFactory` per logical column. The string
/// inside each `GtkStringObject` is unused: the cell strings
/// live in our own per-row map and the factory looks them up by
/// `(rowItem.handle, columnIndex)`.
///
/// `activate` on `GtkColumnView` fires `onActivate` (Enter /
/// double-click); `notify::selected` on the single-selection
/// wrapper fires `onSelect`.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkLabel, GtkBox, GtkOrientation,
    GtkScrolledWindow, GtkFixed,
    GtkStringObject, GtkListItemFactory, GtkListItem,
    GtkColumnView, GtkColumnViewColumn,
    GtkSingleSelection, GListModel, GtkSelectionModel,
    GListStore, GObject,
    gtk_label_new, gtk_label_set_text, gtk_label_set_xalign,
    gtk_scrolled_window_new, gtk_scrolled_window_set_child,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible, gtk_widget_set_tooltip_text,
    gtk_widget_grab_focus,
    gtk_string_object_new,
    gtk_signal_list_item_factory_new,
    gtk_list_item_get_item, gtk_list_item_set_child,
    gtk_single_selection_new, gtk_single_selection_get_selected,
    gtk_single_selection_set_selected, gtk_single_selection_get_selected_item,
    gtk_column_view_new, gtk_column_view_set_model,
    gtk_column_view_append_column, gtk_column_view_column_new,
    gtk_column_view_column_set_fixed_width, gtk_column_view_column_set_title,
    g_list_store_new, g_list_store_append, g_list_store_remove_all,
    g_type_from_name,
    g_signal_connect_factory, g_signal_connect_notify,
    g_signal_connect_activate_u32,
}
use core { resolveGtkParent, stringObjectType }

class TableSlot {
    pub scroll:    GtkScrolledWindow
    pub colView:   GtkColumnView
    pub rowStore:  GListStore
    pub selection: GtkSingleSelection
    pub columns:   GtkColumnViewColumn[]
    pub factories: GtkListItemFactory[]
    pub rows:      string[][]
    pub rowItems:  GtkStringObject[]
    pub fixed:     GtkFixed
    pub winHandle: i64
    pub onSelect:   fn(i32)
    pub onActivate: fn(i32)
    pub onFocus:    fn()
    pub onBlur:     fn()
    pub init(scroll: GtkScrolledWindow, cv: GtkColumnView,
             rs: GListStore, sel: GtkSingleSelection,
             fixed: GtkFixed, winHandle: i64) {
        this.scroll    = scroll
        this.colView   = cv
        this.rowStore  = rs
        this.selection = sel
        this.columns   = []
        this.factories = []
        this.rows      = []
        this.rowItems  = []
        this.fixed     = fixed
        this.winHandle = winHandle
        this.onSelect   = tblNoOpI32
        this.onActivate = tblNoOpI32
        this.onFocus    = tblNoOp
        this.onBlur     = tblNoOp
    }
}

fn tblNoOpI32(i: i32) {}
fn tblNoOp() {}

/// Keyed by the `GtkColumnView`'s GObject pointer.
let TABLES = new Map<i64, TableSlot>()
/// `factory.handle` → packed `(tableKey, colIndex)` — used by
/// `bind` to look up the right cell value. We encode as
/// `tableKey ^ (colIndex << 56)` — `colIndex` always fits in
/// the top byte for the row counts we care about (< 256 cols).
let FACTORY_INFO: Map<i64, i64> = new Map<i64, i64>()
/// `listItem.handle` → the row's `GtkStringObject.handle`, set
/// by `bind` so the unbind callback can drop it. (Currently
/// unused but kept for symmetry with tree.il.)
let ITEM_TO_ROW: Map<i64, i64> = new Map<i64, i64>()
/// `listItem.handle` → the `GtkLabel.handle` we attached in
/// `setup`, so `bind` can update its text without re-walking
/// the widget tree.
let ITEM_TO_LABEL: Map<i64, i64> = new Map<i64, i64>()
/// `rowItem.handle` → owning `tableKey`. Used by `bind` to find
/// the row's cells via the table slot.
let ROW_TO_TABLE: Map<i64, i64> = new Map<i64, i64>()
/// `rowItem.handle` → row index in the table.
let ROW_TO_INDEX: Map<i64, i64> = new Map<i64, i64>()

@extern(C) {

pub fn tableFactorySetup(factory: i64, listItem: i64, packed: i64) {
    let li = listItem as GtkListItem
    let label = gtk_label_new(cstrFromString("")) as GtkLabel
    gtk_label_set_xalign(label, 0.0 as f32)
    gtk_list_item_set_child(li, label as GtkWidget)
    ITEM_TO_LABEL.set(listItem, label as i64)
}

pub fn tableFactoryBind(factory: i64, listItem: i64, packed: i64) {
    let li = listItem as GtkListItem
    let item = gtk_list_item_get_item(li)
    let rh = item as i64
    ITEM_TO_ROW.set(listItem, rh)
    let info: i64 = match FACTORY_INFO.get(factory) {
        some(v) { v }
        none    { 0 as i64 }
    }
    let colIdx: i64 = (info >> 56) & (0xFF as i64)
    if let some(tk) = ROW_TO_TABLE.get(rh) {
        if let some(slot) = TABLES.get(tk) {
            if let some(ri) = ROW_TO_INDEX.get(rh) {
                if ri >= 0 as i64 && ri < slot.rows.length {
                    let cells = slot.rows[ri]
                    let s: string = if colIdx < cells.length { cells[colIdx] } else { "" }
                    if let some(lh) = ITEM_TO_LABEL.get(listItem) {
                        gtk_label_set_text(lh as GtkLabel, cstrFromString(s))
                    }
                }
            }
        }
    }
}

pub fn tableFactoryUnbind(factory: i64, listItem: i64, packed: i64) {
    let _ = ITEM_TO_ROW.delete(listItem)
}

pub fn tableFactoryTeardown(factory: i64, listItem: i64, packed: i64) {
    let _ = ITEM_TO_LABEL.delete(listItem)
}

pub fn tableSelectionChanged(sel: i64, pspec: i64, tableKey: i64) {
    if let some(slot) = TABLES.get(tableKey) {
        let pos = gtk_single_selection_get_selected(slot.selection)
        // GTK_INVALID_LIST_POSITION == 0xFFFFFFFF
        let row: i32 = if pos == 0xFFFFFFFF as u32 {
            -1 as i32
        } else {
            pos as i32
        }
        let cb = slot.onSelect
        cb(row)
    }
}

pub fn tableActivated(cv: i64, position: u32, tableKey: i64) {
    if let some(slot) = TABLES.get(tableKey) {
        let cb = slot.onActivate
        cb(position as i32)
    }
}

pub fn createNativeTable(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let rowStore = g_list_store_new(stringObjectType())
    let sel = gtk_single_selection_new(rowStore as GListModel)
    let cv = gtk_column_view_new(sel as GtkSelectionModel) as GtkColumnView

    let scroll = gtk_scrolled_window_new() as GtkScrolledWindow
    gtk_scrolled_window_set_child(scroll, cv as GtkWidget)
    gtk_widget_set_size_request(scroll as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(cv as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(scroll as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, scroll as GtkWidget, x, y)

    let key = cv as i64
    g_signal_connect_notify(
        sel as GObject, cstrFromString("notify::selected"),
        tableSelectionChanged, key as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_activate_u32(
        cv as GObject, cstrFromString("activate"),
        tableActivated, key as *void, 0 as i64, 0 as u32
    )

    TABLES.set(key, new TableSlot(
        scroll, cv, rowStore, sel, parent.fixed, windowHandle
    ))
    key
}

pub fn tableAddColumn(tableKey: i64, title: string, width: f64) {
    if let some(slot) = TABLES.get(tableKey) {
        let factory = gtk_signal_list_item_factory_new()
        let col = gtk_column_view_column_new(cstrFromString(title), factory)
        gtk_column_view_column_set_fixed_width(col, width as i32)
        gtk_column_view_append_column(slot.colView, col)
        let colIdx = slot.columns.length
        // Pack (tableKey, colIdx). colIdx in the top byte, tableKey
        // in the rest — works while colIdx < 256.
        let packed: i64 = (tableKey & (0x00FFFFFFFFFFFFFF as i64))
                        | ((colIdx & (0xFF as i64)) << 56)
        FACTORY_INFO.set(factory as i64, packed)
        g_signal_connect_factory(
            factory as GObject, cstrFromString("setup"),
            tableFactorySetup, packed as *void, 0 as i64, 0 as u32
        )
        g_signal_connect_factory(
            factory as GObject, cstrFromString("bind"),
            tableFactoryBind, packed as *void, 0 as i64, 0 as u32
        )
        g_signal_connect_factory(
            factory as GObject, cstrFromString("unbind"),
            tableFactoryUnbind, packed as *void, 0 as i64, 0 as u32
        )
        g_signal_connect_factory(
            factory as GObject, cstrFromString("teardown"),
            tableFactoryTeardown, packed as *void, 0 as i64, 0 as u32
        )
        slot.columns.push(col)
        slot.factories.push(factory)
    }
}

pub fn tableAddRow(tableKey: i64, cells: string[]) {
    if let some(slot) = TABLES.get(tableKey) {
        let rowIndex = slot.rows.length
        slot.rows.push(cells)
        let item = gtk_string_object_new(cstrFromString(""))
        slot.rowItems.push(item)
        ROW_TO_TABLE.set(item as i64, tableKey)
        ROW_TO_INDEX.set(item as i64, rowIndex)
        g_list_store_append(slot.rowStore, item as GObject)
    }
}

pub fn clearTableRows(tableKey: i64) {
    if let some(slot) = TABLES.get(tableKey) {
        g_list_store_remove_all(slot.rowStore)
        let n = slot.rowItems.length
        let i: i64 = 0
        while i < n {
            let _ = ROW_TO_TABLE.delete(slot.rowItems[i] as i64)
            let _ = ROW_TO_INDEX.delete(slot.rowItems[i] as i64)
            i = i + 1
        }
        slot.rowItems = []
        slot.rows     = []
    }
}

pub fn readTableSelectedRow(tableKey: i64): i32 {
    match TABLES.get(tableKey) {
        some(slot) {
            let pos = gtk_single_selection_get_selected(slot.selection)
            if pos == 0xFFFFFFFF as u32 { -1 as i32 } else { pos as i32 }
        }
        none { -1 as i32 }
    }
}

pub fn setTableSelectedRow(tableKey: i64, row: i32) {
    if let some(slot) = TABLES.get(tableKey) {
        let pos: u32 = if row < 0 as i32 {
            0xFFFFFFFF as u32
        } else {
            row as u32
        }
        gtk_single_selection_set_selected(slot.selection, pos)
    }
}

pub fn setTableFrame(tableKey: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = TABLES.get(tableKey) {
        gtk_widget_set_size_request(slot.scroll as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.scroll as GtkWidget, x, y)
    }
}

pub fn setTableEnabled(tableKey: i64, enabled: bool) {
    if let some(slot) = TABLES.get(tableKey) {
        gtk_widget_set_sensitive(
            slot.colView as GtkWidget,
            if enabled { 1 as i32 } else { 0 as i32 }
        )
    }
}

pub fn setTableVisible(tableKey: i64, visible: bool) {
    if let some(slot) = TABLES.get(tableKey) {
        gtk_widget_set_visible(
            slot.scroll as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}

pub fn setTableTooltip(tableKey: i64, text: string) {
    if let some(slot) = TABLES.get(tableKey) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.scroll as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.scroll as GtkWidget, cstrFromString(text))
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

// Mouse / key passthrough is stubbed — same Round 1 gap as
// linux/tree.il. The signal-controller plumbing mirrors
// list.il but adds ~200 lines of repetitive surface.
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
        let _ = gtk_widget_grab_focus(slot.colView as GtkWidget)
    }
}
pub fn blurTable(tableKey: i64) {
    let _ = tableKey
}

} // @extern(C)
