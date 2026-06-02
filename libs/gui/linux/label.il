/// GtkLabel wrapper.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkLabel, GtkFixed,
    gtk_label_new, gtk_label_set_text,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_visible,
}
use core { WINDOWS }

class LabelSlot {
    pub label: GtkLabel
    pub fixed: GtkFixed
    pub init(label: GtkLabel, fixed: GtkFixed) {
        this.label = label
        this.fixed = fixed
    }
}

let LABELS = new Map<i64, LabelSlot>()

@extern(C) {

pub fn createNativeLabel(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    visible: bool, selectable: bool
): i64 {
    // `selectable` is honoured on macOS (NSTextField). GtkLabel
    // can expose `set_selectable` too, but we don't bind it yet
    // and the cursor stays as the default arrow regardless.
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let label = gtk_label_new(cstrFromString(text)) as GtkLabel
    gtk_widget_set_size_request(label as GtkWidget, width as i32, height as i32)
    gtk_widget_set_visible(
        label as GtkWidget,
        if visible { 1 as i32 } else { 0 as i32 }
    )
    gtk_fixed_put(winSlot.fixed, label as GtkWidget, x, y)
    let handle = label as i64
    LABELS[handle] = new LabelSlot(label, winSlot.fixed)
    handle
}

pub fn setLabelText(handle: i64, text: string) {
    if let some(slot) = LABELS.get(handle) {
        gtk_label_set_text(slot.label, cstrFromString(text))
    }
}

pub fn setLabelFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = LABELS.get(handle) {
        gtk_widget_set_size_request(slot.label as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.label as GtkWidget, x, y)
    }
}

pub fn setLabelVisible(handle: i64, visible: bool) {
    if let some(slot) = LABELS.get(handle) {
        gtk_widget_set_visible(
            slot.label as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}
pub fn setLabelSelectable(handle: i64, selectable: bool) {
    // No-op for now — gtk_label_set_selectable isn't bound yet.
}

} // @extern(C)
