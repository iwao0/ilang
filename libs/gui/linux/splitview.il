/// Split view — GTK 4 backend. GtkPaned is always 2-pane, so an
/// N-pane splitter materialises as a right-nested chain:
///
///   pane 0 ─┬─ pane 1 ─┬─ pane 2 ─┬─ pane 3 …
///           │          │          │
///         paned0     paned1     paned2
///
/// `paned0` becomes the SplitView's single visible widget; every
/// `addChild` appends a pane by allocating a new GtkFixed (so
/// child widgets keep their absolute-position behaviour inside
/// the pane) and rebuilding the GtkPaned chain to include it.
///
/// `setSplitPosition(i, px)` drives `gtk_paned_set_position` on
/// the i-th `paned` in the chain.

use gtk {
    GtkWidget, GtkFixed, GtkPaned, GtkOrientation, GtkWindow,
    gtk_fixed_new, gtk_fixed_put, gtk_fixed_move,
    gtk_paned_new, gtk_paned_set_start_child, gtk_paned_set_end_child,
    gtk_paned_set_position,
    gtk_widget_unparent, gtk_widget_get_first_child,
    gtk_widget_set_size_request, gtk_widget_set_visible,
    gtk_widget_set_sensitive,
}
use core { WINDOWS, GTK_PARENTS, GtkParent, resolveGtkParent }

pub class SplitViewSlot {
    /// Outer `GtkFixed` placed in the SplitView's parent. The
    /// nested GtkPaned chain (or a single pane fixed when N=1)
    /// becomes its only child.
    pub root:        GtkFixed
    pub panes:       GtkFixed[]
    pub paneds:      GtkPaned[]
    pub orientation: GtkOrientation
    pub parentFixed: GtkFixed
    pub parentWin:   GtkWindow
    pub w:           f64
    pub h:           f64
    pub init(
        root: GtkFixed, orientation: GtkOrientation,
        parentFixed: GtkFixed, parentWin: GtkWindow,
        w: f64, h: f64
    ) {
        this.root        = root
        this.orientation = orientation
        this.parentFixed = parentFixed
        this.parentWin   = parentWin
        this.w           = w
        this.h           = h
        let p: GtkFixed[]  = []
        this.panes  = p
        let q: GtkPaned[]  = []
        this.paneds = q
    }
}

pub let SPLIT_VIEWS = new Map<i64, SplitViewSlot>()

@extern(C) {

/// Discard the previous nested chain and rebuild it from
/// `slot.panes`. Both the existing chain and every pane fixed
/// are unparented first; then a right-nested chain of GtkPaneds
/// is built and parked inside `slot.root`.
fn rebuildChain(slot: SplitViewSlot) {
    // Walk root's child list and unparent each — the chain owns
    // every nested GtkPaned, so dropping its first child cascades
    // the GtkPaned hierarchy out of the widget tree.
    let child = gtk_widget_get_first_child(slot.root as GtkWidget)
    while child as i64 != 0 as i64 {
        gtk_widget_unparent(child)
        child = gtk_widget_get_first_child(slot.root as GtkWidget)
    }
    // Re-unparent every pane too (in case it was previously
    // inside a now-orphaned GtkPaned). GTK is tolerant of
    // already-unparented widgets.
    let n = slot.panes.length
    let i = 0 as i64
    while i < n {
        let p = slot.panes[i] as GtkWidget
        let parent = gtk_widget_get_first_child(p)
        // touching get_first_child only to keep the inner panes'
        // contents addressable through GTK_PARENTS — actual
        // unparenting is unconditional.
        let _ = parent
        gtk_widget_unparent(p)
        i = i + 1
    }
    // Reset the cached GtkPaned list — they're stale handles
    // pointing at GObjects that just lost their last reference.
    let q: GtkPaned[] = []
    slot.paneds = q

    if n == 0 as i64 { return }
    let intW = slot.w as i32
    let intH = slot.h as i32

    if n == 1 as i64 {
        gtk_widget_set_size_request(slot.panes[0] as GtkWidget, intW, intH)
        gtk_fixed_put(slot.root, slot.panes[0] as GtkWidget, 0.0, 0.0)
        return
    }

    // n >= 2: build right-nested GtkPaned chain.
    // Work outermost-in: paned[0] hosts pane[0] on the left, the
    // sub-chain on the right; paned[1] hosts pane[1] + sub-chain;
    // last paned hosts pane[n-2] + pane[n-1].
    let paneds: GtkPaned[] = []
    let j = 0 as i64
    while j < n - 1 as i64 {
        let p = gtk_paned_new(slot.orientation) as GtkPaned
        paneds.push(p)
        j = j + 1
    }
    // Stitch left children + right children top-down.
    let k = 0 as i64
    while k < n - 1 as i64 {
        gtk_paned_set_start_child(paneds[k], slot.panes[k] as GtkWidget)
        if k == n - 2 as i64 {
            gtk_paned_set_end_child(paneds[k], slot.panes[n - 1 as i64] as GtkWidget)
        } else {
            gtk_paned_set_end_child(paneds[k], paneds[k + 1 as i64] as GtkWidget)
        }
        k = k + 1
    }
    slot.paneds = paneds
    gtk_widget_set_size_request(paneds[0] as GtkWidget, intW, intH)
    gtk_fixed_put(slot.root, paneds[0] as GtkWidget, 0.0, 0.0)
}

fn orientFor(value: i32): GtkOrientation {
    // SplitOrientation.horizontal = 0 → panes side-by-side, which
    // GTK calls GTK_ORIENTATION_HORIZONTAL (= 0). Same numeric
    // mapping, but we route it through the enum so the binding's
    // type is preserved.
    if value == 0 as i32 {
        GtkOrientation.horizontal
    } else {
        GtkOrientation.vertical
    }
}

pub fn createNativeSplitView(
    parentHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool, orientation: i32
): i64 {
    let parent = match resolveGtkParent(parentHandle) {
        some(p) { p }
        none    { return -1 }
    }
    let root = gtk_fixed_new() as GtkFixed
    gtk_widget_set_size_request(root as GtkWidget, width as i32, height as i32)
    gtk_widget_set_visible(
        root as GtkWidget,
        if visible { 1 as i32 } else { 0 as i32 }
    )
    gtk_widget_set_sensitive(
        root as GtkWidget,
        if enabled { 1 as i32 } else { 0 as i32 }
    )
    gtk_fixed_put(parent.fixed, root as GtkWidget, x, y)

    let handle = root as i64
    SPLIT_VIEWS[handle] = new SplitViewSlot(
        root, orientFor(orientation), parent.fixed, parent.win, width, height
    )
    handle
}

/// Hook called by the cross-platform `SplitView.addChild` just
/// before the child's `attachTo` runs. Allocates a fresh pane
/// `GtkFixed`, rebuilds the GtkPaned chain to include it, and
/// publishes the pane's fixed as a `GtkParent` so the child's
/// `createNative*` call resolves to it.
pub fn splitViewBeginNewPane(handle: i64): i64 {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        let pane = gtk_fixed_new() as GtkFixed
        slot.panes.push(pane)
        rebuildChain(slot)
        GTK_PARENTS[pane as i64] = new GtkParent(pane, slot.parentWin)
        return pane as i64
    }
    return -1 as i64
}

pub fn splitViewDocumentHandle(handle: i64): i64 {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        let n = slot.panes.length
        if n > 0 as i64 {
            return slot.panes[n - 1 as i64] as i64
        }
        // Before any panes exist, returning the root means the
        // first child realises directly inside the placeholder
        // GtkFixed — addChild then re-runs the chain.
        return slot.root as i64
    }
    return -1 as i64
}

pub fn setSplitViewFrame(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        slot.w = w
        slot.h = h
        gtk_widget_set_size_request(slot.root as GtkWidget, w as i32, h as i32)
        gtk_fixed_move(slot.parentFixed, slot.root as GtkWidget, x, y)
        // Resize the inner chain root too so subsequent paneds
        // pick up the new size.
        let n = slot.panes.length
        if n == 1 as i64 {
            gtk_widget_set_size_request(slot.panes[0] as GtkWidget, w as i32, h as i32)
        }
        if n >= 2 as i64 && slot.paneds.length > 0 as i64 {
            gtk_widget_set_size_request(
                slot.paneds[0] as GtkWidget, w as i32, h as i32
            )
        }
    }
}

pub fn setSplitViewVisible(handle: i64, visible: bool) {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        gtk_widget_set_visible(
            slot.root as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}

pub fn setSplitViewEnabled(handle: i64, enabled: bool) {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        gtk_widget_set_sensitive(
            slot.root as GtkWidget,
            if enabled { 1 as i32 } else { 0 as i32 }
        )
    }
}

pub fn setSplitViewOrientation(handle: i64, orientation: i32) {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        slot.orientation = orientFor(orientation)
        rebuildChain(slot)
    }
}

pub fn setSplitViewPosition(handle: i64, dividerIndex: i32, px: f64) {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        let i = dividerIndex as i64
        if i < 0 as i64 || i >= slot.paneds.length { return }
        gtk_paned_set_position(slot.paneds[i], px as i32)
    }
}

} // @extern(C)
