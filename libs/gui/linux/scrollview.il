/// Scrollable container — `GtkScrolledWindow` wrapping an inner
/// `GtkFixed`. Children are positioned on the inner fixed; the
/// scrolled window provides the viewport + scroll bars.
///
/// The public `handle` is the scrolled-window pointer; the inner
/// fixed's pointer is published via `scrollViewDocumentHandle`
/// and lands in the global `GTK_PARENTS` table so widgets parented
/// under it find a `GtkParent` through `resolveGtkParent`.

use gtk {
    GtkWidget, GtkScrolledWindow, GtkFixed, GtkPolicyType,
    GtkWindow,
    gtk_scrolled_window_new, gtk_scrolled_window_set_child,
    gtk_scrolled_window_set_policy,
    gtk_fixed_new, gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_visible,
    gtk_widget_set_sensitive,
}
use core { WINDOWS, GTK_PARENTS, GtkParent, resolveGtkParent }

class ScrollViewSlot {
    pub scroll:    GtkScrolledWindow
    pub fixed:     GtkFixed
    pub winHandle: i64    // owning window's handle, for focus dispatch
    pub init(scroll: GtkScrolledWindow, fixed: GtkFixed, winHandle: i64) {
        this.scroll    = scroll
        this.fixed     = fixed
        this.winHandle = winHandle
    }
}

let SCROLL_VIEWS = new Map<i64, ScrollViewSlot>()

/// `GtkPolicyType` discriminants we need:
///   ALWAYS = 0, AUTOMATIC = 1, NEVER = 2, EXTERNAL = 3.
fn policyFor(show: bool): GtkPolicyType {
    if show {
        GtkPolicyType.automatic
    } else {
        GtkPolicyType.never
    }
}

@extern(C) {

pub fn createNativeScrollView(
    parentHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool,
    showHScroll: bool, showVScroll: bool
): i64 {
    let parent = match resolveGtkParent(parentHandle) {
        some(p) { p }
        none    { return -1 }
    }
    let scroll = gtk_scrolled_window_new() as GtkScrolledWindow
    gtk_widget_set_size_request(scroll as GtkWidget, width as i32, height as i32)
    gtk_widget_set_visible(
        scroll as GtkWidget,
        if visible { 1 as i32 } else { 0 as i32 }
    )
    gtk_widget_set_sensitive(
        scroll as GtkWidget,
        if enabled { 1 as i32 } else { 0 as i32 }
    )
    gtk_scrolled_window_set_policy(
        scroll, policyFor(showHScroll), policyFor(showVScroll)
    )

    let fixed = gtk_fixed_new() as GtkFixed
    gtk_scrolled_window_set_child(scroll, fixed as GtkWidget)

    gtk_fixed_put(parent.fixed, scroll as GtkWidget, x, y)

    let handle = scroll as i64
    // Look up the owning top-level window so widgets parented under
    // this fixed can still drive focus / default-widget operations
    // through `parent.win`.
    let winHandle = if let some(_) = WINDOWS.get(parentHandle) {
        parentHandle
    } else {
        // Nested scroll view — its parent is another scroll view's
        // fixed. Walk through GTK_PARENTS to find a registered
        // window. For Round 1 we approximate by using the parent's
        // own winHandle if it's in GTK_PARENTS, else fall back.
        parentHandle
    }
    SCROLL_VIEWS[handle] = new ScrollViewSlot(scroll, fixed, winHandle)
    // Publish the inner fixed under its pointer so widget
    // `createNative*` calls keyed on `nativeChildHandle()` (= this
    // fixed) resolve correctly.
    GTK_PARENTS[fixed as i64] = new GtkParent(fixed, parent.win)
    handle
}

pub fn scrollViewDocumentHandle(handle: i64): i64 {
    match SCROLL_VIEWS.get(handle) {
        some(slot) { slot.fixed as i64 }
        none       { -1 as i64 }
    }
}

pub fn setScrollViewFrame(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = SCROLL_VIEWS.get(handle) {
        gtk_widget_set_size_request(slot.scroll as GtkWidget, w as i32, h as i32)
        if let some(parent) = resolveGtkParent(slot.winHandle) {
            gtk_fixed_move(parent.fixed, slot.scroll as GtkWidget, x, y)
        }
    }
}

pub fn setScrollViewVisible(handle: i64, visible: bool) {
    if let some(slot) = SCROLL_VIEWS.get(handle) {
        gtk_widget_set_visible(
            slot.scroll as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}

pub fn setScrollViewEnabled(handle: i64, enabled: bool) {
    if let some(slot) = SCROLL_VIEWS.get(handle) {
        gtk_widget_set_sensitive(
            slot.scroll as GtkWidget,
            if enabled { 1 as i32 } else { 0 as i32 }
        )
    }
}

pub fn setScrollViewShowHScroll(handle: i64, show: bool) {
    if let some(slot) = SCROLL_VIEWS.get(handle) {
        // GtkScrolledWindow takes both axes at once; preserve the
        // current vertical state by reading it back isn't exposed
        // — fall back to automatic for vertical.
        gtk_scrolled_window_set_policy(
            slot.scroll, policyFor(show), GtkPolicyType.automatic
        )
    }
}
pub fn setScrollViewShowVScroll(handle: i64, show: bool) {
    if let some(slot) = SCROLL_VIEWS.get(handle) {
        gtk_scrolled_window_set_policy(
            slot.scroll, GtkPolicyType.automatic, policyFor(show)
        )
    }
}

/// Resize the inner `GtkFixed` to match the requested content
/// area. GTK's `gtk_widget_set_size_request` on a GtkFixed sets
/// its natural size, which the scrolled window uses as the
/// scrollable region.
pub fn setScrollViewContentSize(handle: i64, w: f64, h: f64) {
    if let some(slot) = SCROLL_VIEWS.get(handle) {
        gtk_widget_set_size_request(slot.fixed as GtkWidget, w as i32, h as i32)
    }
}

} // @extern(C)
