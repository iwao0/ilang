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
/// Splitter geometry is driven by each pane's `size_request`, NOT
/// by `gtk_paned_set_position`: in a right-nested chain only the
/// innermost GtkPaned (whose end child is a leaf) honours its
/// position — every *outer* GtkPaned ignores it and hands its
/// start child the whole width, leaving the end sub-chain to paint
/// on top (so all but one pane vanish). Requesting an explicit
/// width per pane is the lever GtkPaned does respect at every
/// level, so `applyPaneSizes` sizes the panes from the divider
/// positions (defaulting to an equal split) and the position is
/// set too as a belt-and-suspenders hint for the innermost paned.

use gtk {
    GtkWidget, GtkFixed, GtkPaned, GtkOrientation, GtkWindow,
    gtk_fixed_new, gtk_fixed_put, gtk_fixed_move,
    gtk_paned_new, gtk_paned_set_start_child, gtk_paned_set_end_child,
    gtk_paned_set_position,
    gtk_widget_unparent, gtk_widget_get_first_child, gtk_widget_get_parent,
    gtk_widget_set_size_request, gtk_widget_set_visible,
    gtk_widget_set_sensitive,
    GObject, g_object_ref, g_object_unref,
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
    /// `true` when panes sit side-by-side (splitter bars vertical).
    /// Cached as a bool so `applyPaneSizes` can pick the main axis
    /// without comparing the GtkOrientation enum.
    pub horizontal:  bool
    pub parentFixed: GtkFixed
    pub parentWin:   GtkWindow
    pub w:           f64
    pub h:           f64
    /// Absolute divider positions along the main axis, in pixels
    /// from the splitter's start. Entry `i` is the boundary between
    /// pane `i` and pane `i + 1`. A `0.0` (or missing) entry means
    /// "auto" — `applyPaneSizes` spreads those panes evenly.
    pub splits:      f64[]
    pub init(
        root: GtkFixed, orientation: GtkOrientation, horizontal: bool,
        parentFixed: GtkFixed, parentWin: GtkWindow,
        w: f64, h: f64
    ) {
        this.root        = root
        this.orientation = orientation
        this.horizontal  = horizontal
        this.parentFixed = parentFixed
        this.parentWin   = parentWin
        this.w           = w
        this.h           = h
        let p: GtkFixed[]  = []
        this.panes  = p
        let q: GtkPaned[]  = []
        this.paneds = q
        let s: f64[]       = []
        this.splits = s
    }
}

pub let SPLIT_VIEWS = new Map<i64, SplitViewSlot>()

@extern(C) {

/// Boundary (absolute main-axis pixel) to the right of pane `i`.
/// Uses the user-supplied split when present and positive, else an
/// even division of `total` across the `n` panes.
fn boundaryAt(slot: SplitViewSlot, i: i64, total: f64, n: i64): f64 {
    if i < slot.splits.length && slot.splits[i] > 0.0 {
        return slot.splits[i]
    }
    return total * ((i + 1 as i64) as f64) / (n as f64)
}

/// Size every pane along the main axis from the divider positions
/// and stretch it fully across the cross axis. This is what makes
/// the nested GtkPaned chain lay out correctly — see the file
/// header for why `gtk_paned_set_position` alone is not enough.
fn applyPaneSizes(slot: SplitViewSlot) {
    let n = slot.panes.length
    if n == 0 as i64 { return }
    let total = if slot.horizontal { slot.w } else { slot.h }
    let cross = if slot.horizontal { slot.h } else { slot.w }
    let crossI = cross as i32
    let ndiv = n - 1 as i64

    let prev = 0.0
    let i = 0 as i64
    while i < n {
        let right = if i < ndiv { boundaryAt(slot, i, total, n) } else { total }
        let width = right - prev
        let wpx = if width < 1.0 { 1 as i32 } else { width as i32 }
        if slot.horizontal {
            gtk_widget_set_size_request(slot.panes[i] as GtkWidget, wpx, crossI)
        } else {
            gtk_widget_set_size_request(slot.panes[i] as GtkWidget, crossI, wpx)
        }
        // paned[i].start is pane[i]; its relative divider position
        // is therefore pane[i]'s own extent. Honoured by the
        // innermost paned; ignored (harmlessly) by the outer ones,
        // which the size_request above already pins.
        if i < ndiv && i < slot.paneds.length {
            gtk_paned_set_position(slot.paneds[i], wpx)
        }
        prev = right
        i = i + 1
    }
}

/// Discard the previous nested chain and rebuild it from
/// `slot.panes`. Both the existing chain and every pane fixed
/// are unparented first; then a right-nested chain of GtkPaneds
/// is built and parked inside `slot.root`.
fn rebuildChain(slot: SplitViewSlot) {
    let n = slot.panes.length

    // Keep every pane alive across the teardown below. A pane's
    // only reference is held by its parent GtkPaned, so unparenting
    // the chain root finalizes the GtkPaned hierarchy *and* the pane
    // GtkFixeds it owns (taking the user widgets inside them with
    // it) — leaving slot.panes full of dangling pointers. An extra
    // ref bridges the rebuild; it's released once each pane is back
    // in the freshly built chain.
    let r = 0 as i64
    while r < n {
        let _ = g_object_ref(slot.panes[r] as GObject)
        r = r + 1
    }

    // Walk root's child list and unparent each — the chain owns
    // every nested GtkPaned, so dropping its first child cascades
    // the GtkPaned hierarchy out of the widget tree.
    let child = gtk_widget_get_first_child(slot.root as GtkWidget)
    while child as i64 != 0 as i64 {
        gtk_widget_unparent(child)
        child = gtk_widget_get_first_child(slot.root as GtkWidget)
    }
    // Finalizing the old GtkPaneds already unparented the panes;
    // unparent any straggler that somehow still has a parent so it
    // can be slotted into the new chain cleanly. The protective refs
    // above guarantee none of these are dangling.
    let i = 0 as i64
    while i < n {
        let p = slot.panes[i] as GtkWidget
        if gtk_widget_get_parent(p) as i64 != 0 as i64 {
            gtk_widget_unparent(p)
        }
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
        g_object_unref(slot.panes[0] as GObject)
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

    // Each pane is now owned by the new chain — drop the protective
    // refs taken at the top.
    let u = 0 as i64
    while u < n {
        g_object_unref(slot.panes[u] as GObject)
        u = u + 1
    }

    // Size the panes from the divider positions now that the chain
    // exists; this is what actually drives the layout.
    applyPaneSizes(slot)
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
        root, orientFor(orientation), orientation == 0 as i32,
        parent.fixed, parent.win, width, height
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
        // Re-spread the panes for the new extent.
        applyPaneSizes(slot)
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
        slot.horizontal  = orientation == 0 as i32
        rebuildChain(slot)
    }
}

pub fn setSplitViewPosition(handle: i64, dividerIndex: i32, px: f64) {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        let i = dividerIndex as i64
        if i < 0 as i64 { return }
        // Grow the splits list so entry `i` lands at the right slot;
        // pad intermediate gaps with 0.0 ("auto").
        while slot.splits.length <= i {
            slot.splits.push(0.0)
        }
        slot.splits[i] = px
        applyPaneSizes(slot)
    }
}

} // @extern(C)
