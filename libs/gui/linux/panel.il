/// Panel — plain `GtkFixed` host registered in `GTK_PARENTS` so
/// widgets parented under it find a `GtkParent` through
/// `resolveGtkParent`.

use gtk {
    GtkWidget, GtkFixed,
    gtk_fixed_new, gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_visible,
}
use core { WINDOWS, GTK_PARENTS, GtkParent, resolveGtkParent }

class PanelSlot {
    pub fixed:     GtkFixed
    pub winHandle: i64
    pub init(fixed: GtkFixed, winHandle: i64) {
        this.fixed     = fixed
        this.winHandle = winHandle
    }
}

let PANELS = new Map<i64, PanelSlot>()

@extern(C) {

pub fn createNativePanel(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(p) { p }
        none    { return -1 }
    }
    let fixed = gtk_fixed_new() as GtkFixed
    gtk_widget_set_size_request(fixed as GtkWidget, width as i32, height as i32)
    gtk_widget_set_visible(
        fixed as GtkWidget,
        if visible { 1 as i32 } else { 0 as i32 }
    )
    gtk_fixed_put(parent.fixed, fixed as GtkWidget, x, y)

    let handle = fixed as i64
    let winHandle = if let some(_) = WINDOWS.get(windowHandle) {
        windowHandle
    } else {
        windowHandle
    }
    PANELS[handle] = new PanelSlot(fixed, winHandle)
    GTK_PARENTS[handle] = new GtkParent(fixed, parent.win)
    handle
}

pub fn setPanelFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = PANELS.get(handle) {
        gtk_widget_set_size_request(
            slot.fixed as GtkWidget, width as i32, height as i32
        )
        if let some(parent) = resolveGtkParent(slot.winHandle) {
            gtk_fixed_move(parent.fixed, slot.fixed as GtkWidget, x, y)
        }
    }
}

pub fn setPanelVisible(handle: i64, visible: bool) {
    if let some(slot) = PANELS.get(handle) {
        gtk_widget_set_visible(
            slot.fixed as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}

} // @extern(C)
