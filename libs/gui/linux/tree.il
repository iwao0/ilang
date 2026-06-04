/// Tree (hierarchical list) — `GtkListView` over a
/// `GtkTreeListModel` that wraps a `GListStore<GtkStringObject>`.
/// Each tree node is identified by its `GtkStringObject` handle
/// (a fresh GObject per node, so the pointer is unique). The
/// per-node `NodeSlot` lives in `NODES` keyed off that handle.
///
/// Row layout: `GtkTreeExpander` containing a horizontal `GtkBox`
/// with an optional `GtkCheckButton` (visible only on
/// `checkable=true` nodes) and a `GtkLabel` for the title.
///
/// The `GtkTreeListModelCreateModelFunc` shape doesn't allow
/// closures, so every callback finds its tree by the `data`
/// pointer (the tree's `GtkListView` handle, cast to `i64`) and
/// the `NodeSlot` by the item's `handle`.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkBox, GtkLabel, GtkCheckButton, GtkOrientation,
    GtkScrolledWindow, GtkFixed,
    GtkStringObject, GtkListItemFactory, GtkListItem, GtkListView,
    GtkTreeListModel, GtkTreeListRow, GtkTreeExpander,
    GtkSingleSelection, GListModel, GtkSelectionModel,
    GListStore, GObject,
    gtk_label_new, gtk_label_set_text, gtk_label_set_xalign,
    gtk_box_new, gtk_box_append,
    gtk_check_button_new, gtk_check_button_set_active,
    gtk_check_button_get_active,
    gtk_scrolled_window_new, gtk_scrolled_window_set_child,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible, gtk_widget_set_tooltip_text,
    gtk_widget_grab_focus,
    gtk_string_object_new, gtk_string_object_get_string,
    gtk_signal_list_item_factory_new,
    gtk_list_item_get_item, gtk_list_item_set_child, gtk_list_item_get_child,
    gtk_tree_expander_new, gtk_tree_expander_set_child,
    gtk_tree_expander_set_list_row, gtk_tree_expander_get_list_row,
    gtk_tree_list_row_get_item, gtk_tree_list_row_get_expanded,
    gtk_tree_list_row_set_expanded, gtk_tree_list_row_is_expandable,
    gtk_tree_list_model_new,
    gtk_single_selection_new, gtk_single_selection_get_selected,
    gtk_single_selection_set_selected, gtk_single_selection_get_selected_item,
    gtk_list_view_new, gtk_list_view_set_model, gtk_list_view_set_factory,
    g_list_store_new, g_list_store_append, g_list_store_insert,
    g_list_store_remove, g_list_store_remove_all,
    g_object_ref, g_object_unref,
    g_type_from_name,
    g_signal_connect_data, g_signal_connect_factory, g_signal_connect_notify,
}
use core { resolveGtkParent }

/// Per-node bookkeeping. The `handle` we hand the public side is
/// the `GtkStringObject` pointer of `item`; `children` is the
/// GListStore<GtkStringObject> we built for this node's
/// immediate descendants (always non-null, even when the node
/// has no children today — keeps the lazy `create_func` simple).
class NodeSlot {
    pub item:      GtkStringObject
    pub label:     string
    pub children:  GListStore
    pub parent:    i64        // parent NodeItem handle (or -1 for top-level)
    pub treeKey:   i64
    pub checkable: bool
    pub checked:   bool
    pub init(item: GtkStringObject, label: string, children: GListStore,
             parent: i64, treeKey: i64, checkable: bool) {
        this.item      = item
        this.label     = label
        this.children  = children
        this.parent    = parent
        this.treeKey   = treeKey
        this.checkable = checkable
        this.checked   = false
    }
}

class TreeSlot {
    pub scroll:    GtkScrolledWindow
    pub listView:  GtkListView
    pub rootStore: GListStore
    pub treeModel: GtkTreeListModel
    pub selection: GtkSingleSelection
    pub factory:   GtkListItemFactory
    pub fixed:     GtkFixed
    pub winHandle: i64
    pub onSelect:   fn(i64)
    pub onExpand:   fn(i64)
    pub onCollapse: fn(i64)
    pub onCheck:    fn(i64, bool)
    pub onFocus:    fn()
    pub onBlur:     fn()
    pub init(scroll: GtkScrolledWindow, lv: GtkListView,
             root: GListStore, tlm: GtkTreeListModel,
             sel: GtkSingleSelection, factory: GtkListItemFactory,
             fixed: GtkFixed, winHandle: i64) {
        this.scroll    = scroll
        this.listView  = lv
        this.rootStore = root
        this.treeModel = tlm
        this.selection = sel
        this.factory   = factory
        this.fixed     = fixed
        this.winHandle = winHandle
        this.onSelect   = noOpI64
        this.onExpand   = noOpI64
        this.onCollapse = noOpI64
        this.onCheck    = noOpI64Bool
        this.onFocus    = treeNoOp
        this.onBlur     = treeNoOp
    }
}

fn noOpI64(h: i64) {}
fn noOpI64Bool(h: i64, v: bool) {}
fn treeNoOp() {}

/// Keyed by the `GtkListView`'s GObject pointer.
let TREES = new Map<i64, TreeSlot>()
/// `GtkStringObject.handle` → its `NodeSlot`.
let NODES: Map<i64, NodeSlot> = new Map<i64, NodeSlot>()
/// `GtkListItem.handle` → currently-bound node's `GtkStringObject.handle`,
/// so the check-button's `toggled` handler can resolve back from
/// the live row.
let ITEM_TO_NODE: Map<i64, i64> = new Map<i64, i64>()
/// `GtkCheckButton.handle` → owning `GtkListItem.handle`. Recreated
/// inside `setup`; the binding step keys off this to find which
/// node the check toggle is for.
let CHECK_TO_ITEM: Map<i64, i64> = new Map<i64, i64>()
/// `GtkListItem.handle` → its row's `GtkLabel.handle` /
/// `GtkCheckButton.handle` so `bind` can reach them without
/// re-traversing the widget hierarchy.
let ITEM_LABELS: Map<i64, i64> = new Map<i64, i64>()
let ITEM_CHECKS: Map<i64, i64> = new Map<i64, i64>()
let ITEM_EXPANDERS: Map<i64, i64> = new Map<i64, i64>()
/// `GtkTreeListRow.handle` → its associated node, so the row's
/// `notify::expanded` handler knows which node fired.
let ROW_TO_NODE: Map<i64, i64> = new Map<i64, i64>()

/// `GObject` type id for "GObject" — `g_list_store_new` wants a
/// GType. We register the store with `GtkStringObject`'s type so
/// it accepts the items we push.
fn stringObjectType(): u64 {
    g_type_from_name(cstrFromString("GtkStringObject"))
}

@extern(C) {

pub fn tree_create_model_for_item(item: i64, treeKey: i64): i64 {
    if let some(slot) = NODES.get(item) {
        // Hand a fresh ref to GTK — GtkTreeListModel will take
        // ownership of the returned model. We keep our own ref
        // in `slot.children`.
        let _ = g_object_ref(slot.children as GObject)
        return slot.children as i64
    }
    0 as i64
}

pub fn tree_factory_setup(factory: i64, listItem: i64, treeKey: i64) {
    let li = listItem as GtkListItem
    let expander = gtk_tree_expander_new() as GtkTreeExpander
    let row = gtk_box_new(GtkOrientation.horizontal, 4 as i32) as GtkBox
    let check = gtk_check_button_new() as GtkCheckButton
    gtk_widget_set_visible(check as GtkWidget, 0 as i32)
    gtk_box_append(row, check as GtkWidget)
    let label = gtk_label_new(cstrFromString("")) as GtkLabel
    gtk_label_set_xalign(label, 0.0 as f32)
    gtk_box_append(row, label as GtkWidget)
    gtk_tree_expander_set_child(expander, row as GtkWidget)
    gtk_list_item_set_child(li, expander as GtkWidget)

    ITEM_LABELS.set(listItem, label as i64)
    ITEM_CHECKS.set(listItem, check as i64)
    ITEM_EXPANDERS.set(listItem, expander as i64)
    CHECK_TO_ITEM.set(check as i64, listItem)

    g_signal_connect_data(
        check as GObject, cstrFromString("toggled"),
        tree_check_toggled, treeKey as *void, 0 as i64, 0 as u32
    )
}

pub fn tree_factory_bind(factory: i64, listItem: i64, treeKey: i64) {
    let li = listItem as GtkListItem
    let rowObj = gtk_list_item_get_item(li)
    let row = rowObj as GtkTreeListRow
    let item = gtk_tree_list_row_get_item(row) as GtkStringObject
    let nh = item as i64

    ITEM_TO_NODE.set(listItem, nh)
    ROW_TO_NODE.set(row as i64, nh)

    if let some(slot) = NODES.get(nh) {
        if let some(lh) = ITEM_LABELS.get(listItem) {
            gtk_label_set_text(lh as GtkLabel, cstrFromString(slot.label))
        }
        if let some(ch) = ITEM_CHECKS.get(listItem) {
            let cb = ch as GtkCheckButton
            gtk_widget_set_visible(
                cb as GtkWidget,
                if slot.checkable { 1 as i32 } else { 0 as i32 }
            )
            gtk_check_button_set_active(
                cb, if slot.checked { 1 as i32 } else { 0 as i32 }
            )
        }
        if let some(eh) = ITEM_EXPANDERS.get(listItem) {
            gtk_tree_expander_set_list_row(eh as GtkTreeExpander, row)
        }
    }

    // notify::expanded on the row fires whenever the disclosure
    // triangle is clicked. Connect once per bind — GTK keeps the
    // row alive while it's in the model, and disconnect happens
    // implicitly on row dispose.
    g_signal_connect_notify(
        row as GObject, cstrFromString("notify::expanded"),
        tree_row_expanded_changed, treeKey as *void, 0 as i64, 0 as u32
    )
}

pub fn tree_factory_unbind(factory: i64, listItem: i64, treeKey: i64) {
    let _ = ITEM_TO_NODE.delete(listItem)
}

pub fn tree_factory_teardown(factory: i64, listItem: i64, treeKey: i64) {
    let _ = ITEM_LABELS.delete(listItem)
    let _ = ITEM_CHECKS.delete(listItem)
    let _ = ITEM_EXPANDERS.delete(listItem)
}

pub fn tree_check_toggled(check: i64, treeKey: i64) {
    let nh: i64 = match CHECK_TO_ITEM.get(check) {
        some(li) {
            match ITEM_TO_NODE.get(li) {
                some(n) { n }
                none    { -1 as i64 }
            }
        }
        none { -1 as i64 }
    }
    if nh < 0 as i64 { return }
    if let some(slot) = NODES.get(nh) {
        let on = gtk_check_button_get_active(check as GtkCheckButton) != 0 as i32
        slot.checked = on
        if let some(t) = TREES.get(treeKey) {
            let cb = t.onCheck
            cb(nh, on)
        }
    }
}

pub fn tree_row_expanded_changed(row: i64, pspec: i64, treeKey: i64) {
    let nh: i64 = match ROW_TO_NODE.get(row) {
        some(n) { n }
        none    { -1 as i64 }
    }
    if nh < 0 as i64 { return }
    let expanded = gtk_tree_list_row_get_expanded(row as GtkTreeListRow) != 0 as i32
    if let some(t) = TREES.get(treeKey) {
        if expanded {
            let cb = t.onExpand; cb(nh)
        } else {
            let cb = t.onCollapse; cb(nh)
        }
    }
}

pub fn tree_selection_changed(sel: i64, pspec: i64, treeKey: i64) {
    if let some(t) = TREES.get(treeKey) {
        let selObj = gtk_single_selection_get_selected_item(t.selection)
        let nh: i64 = if (selObj as i64) == 0 as i64 {
            -1 as i64
        } else {
            // selObj is a GtkTreeListRow; unwrap to the item.
            let row = selObj as GtkTreeListRow
            let item = gtk_tree_list_row_get_item(row)
            item as i64
        }
        let cb = t.onSelect
        cb(nh)
    }
}

pub fn createNativeTree(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let rootStore = g_list_store_new(stringObjectType())
    // `tree_create_model_for_item` resolves children for any
    // non-root node via `NODES`. We point `user_data` at the
    // tree key — the slot it identifies is filled in just below
    // so the callback always sees `none` until then.
    let treeKey: i64 = 0 as i64
    let tlm = gtk_tree_list_model_new(
        rootStore as GListModel,
        0 as i32, 0 as i32,         // passthrough=false, autoexpand=false
        tree_create_model_for_item,
        treeKey as *void,
        0 as i64
    )
    let sel = gtk_single_selection_new(tlm as GListModel)
    let factory = gtk_signal_list_item_factory_new()
    let lv = gtk_list_view_new(sel as GtkSelectionModel, factory) as GtkListView

    let scroll = gtk_scrolled_window_new() as GtkScrolledWindow
    gtk_scrolled_window_set_child(scroll, lv as GtkWidget)
    gtk_widget_set_size_request(scroll as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(lv as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(scroll as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, scroll as GtkWidget, x, y)

    let key = lv as i64

    g_signal_connect_factory(
        factory as GObject, cstrFromString("setup"),
        tree_factory_setup, key as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_factory(
        factory as GObject, cstrFromString("bind"),
        tree_factory_bind, key as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_factory(
        factory as GObject, cstrFromString("unbind"),
        tree_factory_unbind, key as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_factory(
        factory as GObject, cstrFromString("teardown"),
        tree_factory_teardown, key as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_notify(
        sel as GObject, cstrFromString("notify::selected"),
        tree_selection_changed, key as *void, 0 as i64, 0 as u32
    )

    TREES.set(key, new TreeSlot(
        scroll, lv, rootStore, tlm, sel, factory, parent.fixed, windowHandle
    ))
    key
}

pub fn addTreeNode(
    treeKey: i64, parentNodeHandle: i64, label: string, checkable: bool
): i64 {
    insertTreeNodeAt(treeKey, parentNodeHandle, -1 as i32, label, checkable)
}

pub fn insertTreeNodeAt(
    treeKey: i64, parentNodeHandle: i64, index: i32,
    label: string, checkable: bool
): i64 {
    let item = gtk_string_object_new(cstrFromString(label))
    let children = g_list_store_new(stringObjectType())
    let slot = new NodeSlot(item, label, children, parentNodeHandle, treeKey, checkable)
    let h = item as i64
    NODES.set(h, slot)

    let parentStore: GListStore = if parentNodeHandle < 0 as i64 {
        match TREES.get(treeKey) {
            some(t) { t.rootStore }
            none    { children }
        }
    } else {
        match NODES.get(parentNodeHandle) {
            some(p) { p.children }
            none    { children }
        }
    }
    if index < 0 as i32 {
        g_list_store_append(parentStore, item as GObject)
    } else {
        g_list_store_insert(parentStore, index as u32, item as GObject)
    }
    h
}

pub fn removeTreeNode(treeKey: i64, nodeHandle: i64) {
    if let some(slot) = NODES.get(nodeHandle) {
        let parentStore: GListStore = if slot.parent < 0 as i64 {
            match TREES.get(treeKey) {
                some(t) { t.rootStore }
                none    { slot.children }
            }
        } else {
            match NODES.get(slot.parent) {
                some(p) { p.children }
                none    { slot.children }
            }
        }
        let pos = listStoreIndexOf(parentStore, slot.item)
        if pos >= 0 as i64 {
            g_list_store_remove(parentStore, pos as u32)
        }
        forgetSubtree(nodeHandle)
    }
}

fn listStoreIndexOf(store: GListStore, item: GtkStringObject): i64 {
    // GListStore doesn't expose a search helper, but n_items +
    // get_item come from GListModel — we can scan. For an MVP
    // tree where remove is rare, a linear scan is fine.
    let _ = store
    let _ = item
    // TODO: implement once `g_list_model_get_n_items` /
    // `g_list_model_get_item` are surfaced in the bindings.
    -1 as i64
}

fn forgetSubtree(nodeHandle: i64) {
    if let some(slot) = NODES.get(nodeHandle) {
        let _ = slot
        // Recursive child cleanup would walk `slot.children`
        // via GListModel; deferred along with `listStoreIndexOf`.
        let _ = NODES.delete(nodeHandle)
    }
}

pub fn clearTreeNodes(treeKey: i64) {
    if let some(t) = TREES.get(treeKey) {
        g_list_store_remove_all(t.rootStore)
        // NODES cleanup is a leak today — see TODO above.
    }
}

pub fn setTreeNodeLabel(treeKey: i64, nodeHandle: i64, label: string) {
    if let some(slot) = NODES.get(nodeHandle) {
        slot.label = label
        // The bound GtkLabel updates on the next factory rebind
        // (e.g. when the row scrolls back into view). For an
        // immediate refresh we'd need to walk live items, which
        // would need GListModel iteration helpers not yet bound.
    }
}

pub fn setTreeNodeExpanded(treeKey: i64, nodeHandle: i64, expanded: bool) {
    let _ = treeKey
    let _ = nodeHandle
    let _ = expanded
    // Programmatic expand needs a `GtkTreeListRow` handle for the
    // node, which `GtkTreeListModel` only surfaces by position.
    // Position lookup needs `g_list_model_get_n_items` / get_item
    // — surfaced once those land in the bindings.
}

pub fn setTreeNodeChecked(treeKey: i64, nodeHandle: i64, checked: bool) {
    if let some(slot) = NODES.get(nodeHandle) {
        slot.checked = checked
    }
}

pub fn readTreeSelectedNode(treeKey: i64): i64 {
    match TREES.get(treeKey) {
        some(t) {
            let selObj = gtk_single_selection_get_selected_item(t.selection)
            if (selObj as i64) == 0 as i64 {
                -1 as i64
            } else {
                let row = selObj as GtkTreeListRow
                gtk_tree_list_row_get_item(row) as i64
            }
        }
        none { -1 as i64 }
    }
}

pub fn setTreeSelectedNode(treeKey: i64, nodeHandle: i64) {
    if let some(t) = TREES.get(treeKey) {
        if nodeHandle < 0 as i64 {
            // GTK_INVALID_LIST_POSITION = (guint)-1 deselects.
            gtk_single_selection_set_selected(t.selection, 0xFFFFFFFF as u32)
        }
        // Selecting a specific node by handle needs a model→row
        // index lookup that we can't do without the GListModel
        // iteration helpers — see `setTreeNodeExpanded`.
    }
}

pub fn treeExpandAll(treeKey: i64) {
    let _ = treeKey
}

pub fn treeCollapseAll(treeKey: i64) {
    let _ = treeKey
}

pub fn setTreeFrame(treeKey: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(t) = TREES.get(treeKey) {
        gtk_widget_set_size_request(t.scroll as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(t.fixed, t.scroll as GtkWidget, x, y)
    }
}

pub fn setTreeEnabled(treeKey: i64, enabled: bool) {
    if let some(t) = TREES.get(treeKey) {
        gtk_widget_set_sensitive(
            t.listView as GtkWidget,
            if enabled { 1 as i32 } else { 0 as i32 }
        )
    }
}

pub fn setTreeVisible(treeKey: i64, visible: bool) {
    if let some(t) = TREES.get(treeKey) {
        gtk_widget_set_visible(
            t.scroll as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}

pub fn setTreeTooltip(treeKey: i64, text: string) {
    if let some(t) = TREES.get(treeKey) {
        if text == "" {
            gtk_widget_set_tooltip_text(t.scroll as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(t.scroll as GtkWidget, cstrFromString(text))
        }
    }
}

pub fn setOnTreeSelect(treeKey: i64, cb: fn(i64)) {
    if let some(t) = TREES.get(treeKey) { t.onSelect = cb }
}
pub fn setOnTreeExpand(treeKey: i64, cb: fn(i64)) {
    if let some(t) = TREES.get(treeKey) { t.onExpand = cb }
}
pub fn setOnTreeCollapse(treeKey: i64, cb: fn(i64)) {
    if let some(t) = TREES.get(treeKey) { t.onCollapse = cb }
}
pub fn setOnTreeCheck(treeKey: i64, cb: fn(i64, bool)) {
    if let some(t) = TREES.get(treeKey) { t.onCheck = cb }
}
pub fn setOnTreeFocus(treeKey: i64, cb: fn()) {
    if let some(t) = TREES.get(treeKey) { t.onFocus = cb }
}
pub fn setOnTreeBlur(treeKey: i64, cb: fn()) {
    if let some(t) = TREES.get(treeKey) { t.onBlur = cb }
}

// Mouse/key hooks land here as no-ops — same Round 1 gap as
// cocoa/tree.il. The signal-controller plumbing is straight-
// forward to add later (mirror `linux/list.il`) but adds ~200
// lines of repetitive surface that isn't needed for the
// MVP smoke test.
pub fn setOnTreeMouseEnter(treeKey: i64, cb: fn(f64, f64)) {
    let _ = treeKey
    let _ = cb
}
pub fn setOnTreeMouseLeave(treeKey: i64, cb: fn(f64, f64)) {
    let _ = treeKey
    let _ = cb
}
pub fn setOnTreeMouseDown(
    treeKey: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = treeKey; let _ = cb }
pub fn setOnTreeMouseUp(
    treeKey: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = treeKey; let _ = cb }
pub fn setOnTreeContextMenu(
    treeKey: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = treeKey; let _ = cb }
pub fn setOnTreeMouseWheel(
    treeKey: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) { let _ = treeKey; let _ = cb }
pub fn setOnTreeKeyDown(treeKey: i64, cb: fn(string, bool, bool, bool, bool)) {
    let _ = treeKey
    let _ = cb
}
pub fn setOnTreeKeyUp(treeKey: i64, cb: fn(string, bool, bool, bool, bool)) {
    let _ = treeKey
    let _ = cb
}

pub fn focusTree(treeKey: i64) {
    if let some(t) = TREES.get(treeKey) {
        let _ = gtk_widget_grab_focus(t.listView as GtkWidget)
    }
}
pub fn blurTree(treeKey: i64) {
    let _ = treeKey
}

} // @extern(C)
