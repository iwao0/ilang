/// GtkEntry — single-line text input.

use gtk {
    GtkWidget, GtkEntry, GtkFixed, GObject,
    gtk_entry_new, gtk_editable_set_text, gtk_editable_get_text,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible,
    g_signal_connect_data,
}
use core { WINDOWS, noOpText }

class EditSlot {
    pub entry: GtkEntry
    pub fixed: GtkFixed
    pub onChange: fn(string)
    pub init(entry: GtkEntry, fixed: GtkFixed) {
        this.entry = entry
        this.fixed = fixed
        this.onChange = noOpText
    }
}

let EDITS = new Map<i64, EditSlot>()

@extern(C) {
    pub fn linux_entry_changed(entry: i64, data: i64) {
        let key = entry as i64
        if let some(slot) = EDITS.get(key) {
            let p = gtk_editable_get_text(slot.entry)
            let s = stringFromCstr(p)
            let cb = slot.onChange
            cb(s)
        }
    }
}

@extern(C) {

pub fn createNativeEdit(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let entry = gtk_entry_new()
    gtk_editable_set_text(entry, cstrFromString(text))
    gtk_widget_set_size_request(entry as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(
        entry as GtkWidget,
        if enabled { 1 as i32 } else { 0 as i32 }
    )
    gtk_widget_set_visible(
        entry as GtkWidget,
        if visible { 1 as i32 } else { 0 as i32 }
    )
    gtk_fixed_put(winSlot.fixed, entry as GtkWidget, x, y)

    g_signal_connect_data(
        entry as GObject,
        cstrFromString("changed"),
        linux_entry_changed,
        0 as *void, 0 as i64, 0 as u32
    )

    let handle = entry as i64
    EDITS[handle] = new EditSlot(entry, winSlot.fixed)
    handle
}

pub fn setOnEditChange(handle: i64, cb: fn(string)) {
    if let some(slot) = EDITS.get(handle) { slot.onChange = cb }
}
pub fn setEditText(handle: i64, text: string) {
    if let some(slot) = EDITS.get(handle) {
        gtk_editable_set_text(slot.entry, cstrFromString(text))
    }
}
pub fn readEditText(handle: i64): string {
    match EDITS.get(handle) {
        some(slot) { stringFromCstr(gtk_editable_get_text(slot.entry)) }
        none       { "" }
    }
}
pub fn setEditFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = EDITS.get(handle) {
        gtk_widget_set_size_request(slot.entry as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.entry as GtkWidget, x, y)
    }
}
pub fn setEditEnabled(handle: i64, enabled: bool) {
    if let some(slot) = EDITS.get(handle) {
        gtk_widget_set_sensitive(
            slot.entry as GtkWidget,
            if enabled { 1 as i32 } else { 0 as i32 }
        )
    }
}
pub fn setEditVisible(handle: i64, visible: bool) {
    if let some(slot) = EDITS.get(handle) {
        gtk_widget_set_visible(
            slot.entry as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}

} // @extern(C)
