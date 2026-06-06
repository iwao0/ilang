/// GtkSpinner wrapper — indeterminate circular "loading" spinner.
/// `gtk_spinner_new` maps cleanly onto gui-core's ActivityIndicator:
/// `start()` / `stop()` drive `gtk_spinner_start` / `gtk_spinner_stop`,
/// and the widget sizes itself inside whatever rect we request.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkSpinner, GtkFixed,
    gtk_spinner_new, gtk_spinner_start, gtk_spinner_stop,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request,
    gtk_widget_set_visible, gtk_widget_set_tooltip_text,
}
use core { resolveGtkParent }

class SpinnerSlot {
    pub spinner: GtkSpinner
    pub fixed:   GtkFixed
    pub init(spinner: GtkSpinner, fixed: GtkFixed) {
        this.spinner = spinner
        this.fixed = fixed
    }
}

let SPINNERS = new Map<i64, SpinnerSlot>()

@extern(C) {

pub fn createNativeActivityIndicator(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let spinner = gtk_spinner_new() as GtkSpinner
    gtk_widget_set_size_request(spinner as GtkWidget, width as i32, height as i32)
    gtk_widget_set_visible(spinner as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, spinner as GtkWidget, x, y)
    let handle = spinner as i64
    SPINNERS[handle] = new SpinnerSlot(spinner, parent.fixed)
    handle
}

pub fn startActivityIndicator(handle: i64) {
    if let some(slot) = SPINNERS.get(handle) {
        gtk_spinner_start(slot.spinner)
    }
}
pub fn stopActivityIndicator(handle: i64) {
    if let some(slot) = SPINNERS.get(handle) {
        gtk_spinner_stop(slot.spinner)
    }
}
pub fn setActivityIndicatorFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = SPINNERS.get(handle) {
        gtk_widget_set_size_request(slot.spinner as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.spinner as GtkWidget, x, y)
    }
}
pub fn setActivityIndicatorVisible(handle: i64, visible: bool) {
    if let some(slot) = SPINNERS.get(handle) {
        gtk_widget_set_visible(slot.spinner as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setActivityIndicatorTooltip(handle: i64, text: string) {
    if let some(slot) = SPINNERS.get(handle) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.spinner as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.spinner as GtkWidget, cstrFromString(text))
        }
    }
}

} // @extern(C)
