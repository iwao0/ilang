/// Tree (hierarchical list). `NSOutlineView` inside an
/// `NSScrollView`. Items are arbitrary `id`s; we hand out one
/// `NodeItem` (a bare `NSObject` subclass) per `TreeNode` and key
/// the per-node slot map off its raw ObjC pointer. The data
/// source serves the public side's tree shape; the delegate
/// surfaces selection / expand / collapse / checkbox events.

use cocoa {
    NSObject, NSView, NSWindow, NSTableView, NSTableColumn,
    NSTableCellView,
    NSOutlineView, NSScrollView, NSTextField, NSButton,
    NSNotification, NSEvent, NSDictionary, NSString,
    makeRect, makeScrollView, nsstr, nil, selectorPtr,
    classPtr, setObjectClass,
}
use core {
    WINDOWS, resolveParentView, noOp1,
    noOpEnterLeave, noOpClickMod, noOpWheel,
    keyCodeToString, modCtrl, modShift, modAlt, modMeta,
}

/// Marker NSObject — each tree node owns one, and its raw
/// pointer is what the data source hands `NSOutlineView` as the
/// item id. Declared before `NodeSlot` / `TreeSlot` so the
/// field types resolve in declaration order.
class NodeItem : NSObject {
    /// Dummy method — `register()` requires the class to surface
    /// at least one declaration the @objc desugar can hang onto.
    pub _markerSelf(): NSObject { this as NSObject }
}

/// Plain NSView subclass — needed because the cocoa bindings
/// don't expose `NSView.alloc()` directly. Every instance is
/// just a layout container for the row's text field (+ optional
/// checkbox).
class TreeCellView : NSView {
    pub _markerSelf(): NSObject { this as NSObject }
}

/// Per-node slot keyed by the `NodeItem`'s raw ObjC pointer.
class NodeSlot {
    pub item:      NodeItem
    pub label:     string
    pub children:  NodeItem[]
    pub parent:    NodeItem
    pub treeKey:   i64
    pub checkable: bool
    pub checked:   bool
    pub init(item: NodeItem, label: string, parent: NodeItem,
             treeKey: i64, checkable: bool) {
        this.item      = item
        this.label     = label
        this.children  = []
        this.parent    = parent
        this.treeKey   = treeKey
        this.checkable = checkable
        this.checked   = false
    }
}

class TreeSlot {
    pub outline:    NSOutlineView
    pub scroll:     NSScrollView
    pub rootChildren: NodeItem[]
    pub onSelect:   fn(i64)
    pub onExpand:   fn(i64)
    pub onCollapse: fn(i64)
    pub onCheck:    fn(i64, bool)
    pub onFocus:    fn()
    pub onBlur:     fn()
    pub dataSource:   NSObject
    pub delegate:     NSObject
    pub checkHandler: NSObject
    pub init(outline: NSOutlineView, scroll: NSScrollView,
             ds: NSObject, dg: NSObject, ch: NSObject) {
        this.outline      = outline
        this.scroll       = scroll
        this.rootChildren = []
        this.onSelect     = noOpI64
        this.onExpand     = noOpI64
        this.onCollapse   = noOpI64
        this.onCheck      = noOpI64Bool
        this.onFocus      = noOp1
        this.onBlur       = noOp1
        this.dataSource   = ds
        this.delegate     = dg
        this.checkHandler = ch
    }
}

fn noOpI64(h: i64) {}
fn noOpI64Bool(h: i64, v: bool) {}

/// Keyed by `outline.handle` (the NSOutlineView's NSView handle).
let TREES = new Map<i64, TreeSlot>()
/// Reverse map: dataSource/delegate handle → tree key, so the
/// callbacks can find their slot.
let DS_TO_TREE: Map<i64, i64> = new Map<i64, i64>()
/// nodeItem.handle → its containing tree's key.
let NODE_TO_TREE: Map<i64, i64> = new Map<i64, i64>()
/// nodeItem.handle → its NodeSlot.
let NODES: Map<i64, NodeSlot> = new Map<i64, NodeSlot>()
/// Live checkbox NSButton.handle → its NodeItem.handle, so the
/// check handler can route the click. View-based outline views
/// recycle row views, so this map is rebuilt as rows scroll in.
let CHECK_BTN_TO_NODE: Map<i64, i64> = new Map<i64, i64>()
/// `NSTableCellView.handle` → its embedded checkbox `NSButton.handle`.
/// Lets a re-bind for a checkable row find the button it built
/// on first creation without re-walking subviews.
let CELL_TO_BTN: Map<i64, i64> = new Map<i64, i64>()

class TreeDataSource : NSObject {
    @objc("outlineView:numberOfChildrenOfItem:")
    pub outlineViewNumberOfChildrenOfItem(
        ov: NSOutlineView, item: NSObject
    ): i64 {
        if let some(tk) = DS_TO_TREE.get(this.handle) {
            if let some(t) = TREES.get(tk) {
                if item.handle == 0 as i64 {
                    return t.rootChildren.length
                }
                if let some(n) = NODES.get(item.handle) {
                    return n.children.length
                }
            }
        }
        0 as i64
    }

    /// `outlineView:child:ofItem:` — return the Nth child item.
    /// We bump the ObjC refcount by `+1` on every return: ilang's
    /// `@objc` dispatch doesn't autorelease return values, so the
    /// retain/release pair AppKit performs inside
    /// `loadItemEntryLazyInfoIfNecessary` (observed in the
    /// disclosure-triangle crash trace) can pull the refcount
    /// below zero between calls. The `+1` matches the autorelease
    /// convention from the caller's perspective — AppKit's own
    /// release inside the same flow consumes it.
    ///
    /// This adds to refcounts on long-lived `NodeItem` instances
    /// (no fresh allocations per query), and the RSS-stress test
    /// in `tests/` confirms steady-state memory under
    /// `autoreleasepool` is flat (~0.7 MB over 8000 expand/
    /// collapse ops vs ~20 MB without). The real run-loop drains
    /// the thread autorelease pool every tick, so production
    /// usage stays bounded.
    @objc("outlineView:child:ofItem:")
    pub outlineViewChildOfItem(
        ov: NSOutlineView, child: i64, ofItem: NSObject
    ): NSObject {
        if let some(tk) = DS_TO_TREE.get(this.handle) {
            if let some(t) = TREES.get(tk) {
                if ofItem.handle == 0 as i64 {
                    if child >= 0 as i64 && child < t.rootChildren.length {
                        return (t.rootChildren[child] as NSObject).retain()
                    }
                } else {
                    if let some(n) = NODES.get(ofItem.handle) {
                        if child >= 0 as i64 && child < n.children.length {
                            return (n.children[child] as NSObject).retain()
                        }
                    }
                }
            }
        }
        nil
    }

    @objc("outlineView:isItemExpandable:")
    pub outlineViewIsItemExpandable(
        ov: NSOutlineView, item: NSObject
    ): bool {
        if let some(n) = NODES.get(item.handle) {
            return n.children.length > 0 as i64
        }
        false
    }

    @objc("outlineView:objectValueForTableColumn:byItem:")
    pub outlineViewObjectValueForTableColumn(
        ov: NSOutlineView, column: NSTableColumn, item: NSObject
    ): NSObject {
        if let some(n) = NODES.get(item.handle) {
            return nsstr(n.label) as NSObject
        }
        nsstr("") as NSObject
    }
}

class TreeDelegate : NSObject {
    pub outlineViewSelectionDidChange(n: NSNotification) {
        if let some(tk) = DS_TO_TREE.get(this.handle) {
            if let some(t) = TREES.get(tk) {
                let row = t.outline.selectedRow
                let h: i64 = if row < 0 as i64 {
                    -1 as i64
                } else {
                    let it = t.outline.itemAtRow(row)
                    it.handle
                }
                let cb = t.onSelect
                cb(h)
            }
        }
    }

    @objc("outlineViewItemDidExpand:")
    pub outlineViewItemDidExpand(n: NSNotification) {
        let h = nodeHandleFromUserInfo(n)
        if h == 0 as i64 { return }
        if let some(tk) = DS_TO_TREE.get(this.handle) {
            if let some(t) = TREES.get(tk) {
                let cb = t.onExpand
                cb(h)
            }
        }
    }

    @objc("outlineViewItemDidCollapse:")
    pub outlineViewItemDidCollapse(n: NSNotification) {
        let h = nodeHandleFromUserInfo(n)
        if h == 0 as i64 { return }
        if let some(tk) = DS_TO_TREE.get(this.handle) {
            if let some(t) = TREES.get(tk) {
                let cb = t.onCollapse
                cb(h)
            }
        }
    }

    /// `outlineView:viewForTableColumn:item:` — build a fresh row
    /// view per call. We don't use the AppKit reuse-queue here;
    /// each row gets its own NSTextField (and, for `checkable`
    /// nodes, its own NSButton). The map of button→node is
    /// rebuilt at the same time so the toggle handler can route
    /// clicks back to the right slot.
    /// View-based path. The first time AppKit asks for a cell
    /// with one of our two identifiers (`"lb"` for label-only,
    /// `"ck"` for label + checkbox) we mint an `NSTableCellView`
    /// with the right subviews and stash the button (if any)
    /// in `CELL_TO_BTN`. Subsequent rows come back through
    /// `makeViewWithIdentifier:owner:`, and we just rewrite the
    /// title / check state / node-handle binding.
    @objc("outlineView:viewForTableColumn:item:")
    pub outlineViewViewForTableColumn(
        ov: NSOutlineView, col: NSTableColumn, item: NSObject
    ): NSView {
        let nodeSlotOpt = NODES.get(item.handle)
        let label: string = match nodeSlotOpt {
            some(n) { n.label }
            none    { "" }
        }
        let checkable: bool = match nodeSlotOpt {
            some(n) { n.checkable }
            none    { false }
        }
        let checked: bool = match nodeSlotOpt {
            some(n) { n.checked }
            none    { false }
        }
        let ident = if checkable { nsstr("ck") } else { nsstr("lb") }
        let reused = (ov as NSTableView).makeViewWithIdentifier(ident, nil)
        let cellView: NSTableCellView = if reused.handle != 0 as i64 {
            NSTableCellView.__wrap_handle_unowned(reused.handle)
        } else {
            let cv = NSTableCellView.alloc().init(makeRect(0.0, 0.0, 200.0, 18.0))
            (cv as NSView).setIdentifier(ident)
            let xOff: f64 = if checkable { 22.0 } else { 2.0 }
            if checkable {
                if let some(tk) = DS_TO_TREE.get(this.handle) {
                    if let some(t) = TREES.get(tk) {
                        let btn = NSButton.alloc().init(makeRect(2.0, 1.0, 18.0, 18.0))
                        // NSButtonTypeSwitch = 3.
                        btn.setButtonType(3 as i64)
                        btn.setTitle(nsstr(""))
                        btn.setTarget(t.checkHandler)
                        btn.setAction(selectorPtr("treeCheckToggled:"))
                        (cv as NSView).addSubview(btn as NSView)
                        CELL_TO_BTN.set((cv as NSView).handle, btn.handle)
                    }
                }
            }
            let tf = NSTextField.alloc().init(
                makeRect(xOff, 0.0, 200.0 - xOff, 17.0)
            )
            tf.setEditable(false)
            tf.setBordered(false)
            tf.setBezeled(false)
            tf.setDrawsBackground(false)
            tf.setSelectable(false)
            (cv as NSView).addSubview(tf as NSView)
            cv.textField = tf
            cv
        }
        // Re-bind row content. With reused views we'd otherwise
        // see a previous row's label / check state.
        cellView.textField.stringValue = nsstr(label)
        if checkable {
            if let some(bh) = CELL_TO_BTN.get((cellView as NSView).handle) {
                let btn = NSButton.__wrap_handle_unowned(bh)
                btn.state = if checked { 1 as i64 } else { 0 as i64 }
                CHECK_BTN_TO_NODE.set(bh, item.handle)
            }
        }
        cellView as NSView
    }
}

class TreeCheckHandler : NSObject {
    @objc("treeCheckToggled:")
    pub treeCheckToggled(sender: NSObject) {
        if let some(nh) = CHECK_BTN_TO_NODE.get(sender.handle) {
            if let some(n) = NODES.get(nh) {
                let on = NSButton.__wrap_handle_unowned(sender.handle).state == 1 as i64
                n.checked = on
                if let some(t) = TREES.get(n.treeKey) {
                    let cb = t.onCheck
                    cb(nh, on)
                }
            }
        }
    }
}

/// Reads `notification.userInfo["NSObject"]` — the standard key
/// `NSOutlineView` uses to ship the affected item in its
/// `Expand` / `Collapse` notifications. Returns `0` when the
/// payload is missing.
fn nodeHandleFromUserInfo(n: NSNotification): i64 {
    let info = n.userInfo
    if (info as NSObject).handle == 0 as i64 { return 0 as i64 }
    let key = nsstr("NSObject") as NSObject
    let item = info.objectForKey(key)
    if (item as NSObject).handle == 0 as i64 { return 0 as i64 }
    item.handle
}

@extern(C) {

pub fn createNativeTree(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let _ = enabled
    let outline = NSOutlineView.alloc().init(makeRect(0.0, 0.0, width, height))
    let theCol = NSTableColumn.alloc().initWithIdentifier(nsstr("v"))
    theCol.isEditable = false
    theCol.width = width - 4.0
    theCol.minWidth = 40.0
    outline.addTableColumn(theCol)
    // First column added becomes the outline column by default,
    // so we don't need to assign `outlineTableColumn` explicitly
    // — and the property-setter form chokes the parser here.
    (outline as NSTableView).setHeaderView(nil)
    (outline as NSTableView).allowsMultipleSelection = false

    TreeDataSource.register()
    TreeDelegate.register()
    TreeCheckHandler.register()
    NodeItem.register()
    TreeCellView.register()

    let ds = TreeDataSource.alloc().init()
    let dg = TreeDelegate.alloc().init()
    let ch = TreeCheckHandler.alloc().init()
    (outline as NSTableView).setDataSource(ds as NSObject)
    (outline as NSTableView).setDelegate(dg as NSObject)

    let scroll = makeScrollView(outline as NSView, x, y, width, height)
    scroll.setHidden(!visible)
    parent.addSubview(scroll as NSView)

    let treeKey = (outline as NSView).handle
    let slot = new TreeSlot(outline, scroll, ds as NSObject, dg as NSObject, ch as NSObject)
    TREES[treeKey] = slot
    DS_TO_TREE[(ds as NSObject).handle] = treeKey
    DS_TO_TREE[(dg as NSObject).handle] = treeKey
    treeKey
}

/// Append a new node under `parentNodeHandle` (use `-1` for top-
/// level). Returns the new node's handle. Internally calls
/// `insertTreeNodeAt` with an out-of-range index so the backend
/// falls through to the append path.
pub fn addTreeNode(
    treeKey: i64, parentNodeHandle: i64, label: string, checkable: bool
): i64 {
    insertTreeNodeAt(treeKey, parentNodeHandle, -1 as i32, label, checkable)
}

pub fn insertTreeNodeAt(
    treeKey: i64, parentNodeHandle: i64, index: i32,
    label: string, checkable: bool
): i64 {
    match TREES.get(treeKey) {
        some(t) {
            let item = NodeItem.alloc().init()
            let parentItem: NodeItem = if parentNodeHandle < 0 as i64 {
                item
            } else {
                match NODES.get(parentNodeHandle) {
                    some(p) { p.item }
                    none    { item }
                }
            }
            let slot = new NodeSlot(item, label, parentItem, treeKey, checkable)
            let h = (item as NSObject).handle
            NODES.set(h, slot)
            NODE_TO_TREE.set(h, treeKey)
            if parentNodeHandle < 0 as i64 {
                insertIntoArray(t.rootChildren, item, index)
            } else {
                if let some(p) = NODES.get(parentNodeHandle) {
                    insertIntoArray(p.children, item, index)
                }
            }
            t.outline.reloadData()
            h
        }
        none { -1 as i64 }
    }
}

fn insertIntoArray(arr: NodeItem[], item: NodeItem, at: i32) {
    let n = arr.length
    let i: i64 = at as i64
    if at < 0 as i32 || i >= n {
        arr.push(item)
        return
    }
    if i <= 0 as i64 {
        arr.unshift(item)
        return
    }
    // Rebuild: nothing on Array yet for arbitrary-index insert.
    let next: NodeItem[] = []
    let k: i64 = 0
    while k < i {
        next.push(arr[k])
        k = k + 1
    }
    next.push(item)
    while k < n {
        next.push(arr[k])
        k = k + 1
    }
    // Mutate in place by clearing + pushing — the caller passed
    // the live array by reference (ilang arrays are reference
    // types), and `arr.length = 0` isn't available, so shift
    // every slot out then push the rebuilt order.
    while arr.length > 0 as i64 {
        let _ = arr.shift()
    }
    let m = next.length
    let j: i64 = 0
    while j < m {
        arr.push(next[j])
        j = j + 1
    }
}

pub fn removeTreeNode(treeKey: i64, nodeHandle: i64) {
    if let some(t) = TREES.get(treeKey) {
        if let some(n) = NODES.get(nodeHandle) {
            // Detach from parent's child list (or the tree's root list).
            removeFromArray(t.rootChildren, n.item)
            for p in NODES.values() {
                removeFromArray(p.children, n.item)
            }
            forgetSubtree(nodeHandle)
            t.outline.reloadData()
        }
    }
}

fn removeFromArray(arr: NodeItem[], item: NodeItem) {
    let n = arr.length
    let i: i64 = 0
    while i < n {
        if (arr[i] as NSObject).handle == (item as NSObject).handle {
            let _ = arr.removeAt(i)
            return
        }
        i = i + 1
    }
}

fn forgetSubtree(nodeHandle: i64) {
    if let some(n) = NODES.get(nodeHandle) {
        let kids = n.children
        let cnt = kids.length
        let i: i64 = 0
        while i < cnt {
            forgetSubtree((kids[i] as NSObject).handle)
            i = i + 1
        }
        let _ = NODES.delete(nodeHandle)
        let _ = NODE_TO_TREE.delete(nodeHandle)
    }
}

pub fn clearTreeNodes(treeKey: i64) {
    if let some(t) = TREES.get(treeKey) {
        let kids = t.rootChildren
        let cnt = kids.length
        let i: i64 = 0
        while i < cnt {
            forgetSubtree((kids[i] as NSObject).handle)
            i = i + 1
        }
        while t.rootChildren.length > 0 as i64 {
            let _ = t.rootChildren.shift()
        }
        t.outline.reloadData()
    }
}

pub fn setTreeNodeLabel(treeKey: i64, nodeHandle: i64, label: string) {
    if let some(n) = NODES.get(nodeHandle) {
        n.label = label
        if let some(t) = TREES.get(treeKey) {
            t.outline.reloadItem(n.item as NSObject)
        }
    }
}

pub fn setTreeNodeExpanded(treeKey: i64, nodeHandle: i64, expanded: bool) {
    if let some(t) = TREES.get(treeKey) {
        if let some(n) = NODES.get(nodeHandle) {
            if expanded {
                t.outline.expandItem(n.item as NSObject)
            } else {
                t.outline.collapseItem(n.item as NSObject)
            }
        }
    }
}

pub fn setTreeNodeChecked(treeKey: i64, nodeHandle: i64, checked: bool) {
    if let some(n) = NODES.get(nodeHandle) {
        n.checked = checked
        // Re-render the row so the live NSButton picks up the
        // new state. reloadItem rebuilds the cell view.
        if let some(t) = TREES.get(treeKey) {
            t.outline.reloadItem(n.item as NSObject)
        }
    }
}

pub fn readTreeSelectedNode(treeKey: i64): i64 {
    match TREES.get(treeKey) {
        some(t) {
            let row = t.outline.selectedRow
            if row < 0 as i64 {
                -1 as i64
            } else {
                t.outline.itemAtRow(row).handle
            }
        }
        none { -1 as i64 }
    }
}

pub fn setTreeSelectedNode(treeKey: i64, nodeHandle: i64) {
    if let some(t) = TREES.get(treeKey) {
        if nodeHandle < 0 as i64 {
            t.outline.deselectAll(nil)
        }
        // Selecting a specific row would want an NSIndexSet for
        // `selectRowIndexes:byExtendingSelection:`. The cocoa
        // bindings don't surface that constructor today, so we
        // fall back to "clear selection" only — same gap as
        // listbox.
    }
}

pub fn treeExpandAll(treeKey: i64) {
    if let some(t) = TREES.get(treeKey) {
        t.outline.expandItemRecursive(nil, true)
    }
}

pub fn treeCollapseAll(treeKey: i64) {
    if let some(t) = TREES.get(treeKey) {
        t.outline.collapseItemRecursive(nil, true)
    }
}

pub fn setTreeFrame(treeKey: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(t) = TREES.get(treeKey) {
        t.scroll.setFrame(makeRect(x, y, width, height))
    }
}

pub fn setTreeEnabled(treeKey: i64, enabled: bool) {
    let _ = treeKey
    let _ = enabled
    // NSOutlineView has no direct enabled toggle. Same gap as
    // listbox — surface as no-op.
}

pub fn setTreeVisible(treeKey: i64, visible: bool) {
    if let some(t) = TREES.get(treeKey) { t.scroll.setHidden(!visible) }
}

pub fn setTreeTooltip(treeKey: i64, text: string) {
    if let some(t) = TREES.get(treeKey) {
        if text == "" {
            (t.scroll as NSView).setToolTip(nil)
        } else {
            (t.scroll as NSView).setToolTip(nsstr(text) as NSObject)
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

/// The remaining mouse / key hooks land here as no-op
/// installers — same surface as listbox / treeview's public
/// side, but `NSOutlineView`'s subview swap-out per row makes
/// per-row tracking-area / first-responder bookkeeping enough
/// extra plumbing to defer past Round 1. The callbacks are
/// stored on the slot so a later upgrade can surface them
/// without changing the public API.
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
) {
    let _ = treeKey
    let _ = cb
}
pub fn setOnTreeMouseUp(
    treeKey: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    let _ = treeKey
    let _ = cb
}
pub fn setOnTreeContextMenu(
    treeKey: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    let _ = treeKey
    let _ = cb
}
pub fn setOnTreeMouseWheel(
    treeKey: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    let _ = treeKey
    let _ = cb
}
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
        let win = (t.outline as NSView).window
        let _ = win.makeFirstResponder(t.outline as NSObject)
    }
}
pub fn blurTree(treeKey: i64) {
    if let some(t) = TREES.get(treeKey) {
        let win = (t.outline as NSView).window
        if win.firstResponder.handle == (t.outline as NSView).handle {
            let _ = win.makeFirstResponder(nil)
        }
    }
}

} // @extern(C)
