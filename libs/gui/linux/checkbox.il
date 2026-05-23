/// GtkCheckButton wrapper — the standalone-checkbox case. The
/// radio case (mutually-exclusive group) lives in radio.il, also
/// backed by GtkCheckButton via `gtk_check_button_set_group`.

use gtk {
    GtkWidget, GtkCheckButton, GtkFixed, GObject,
    gtk_check_button_new_with_label, gtk_check_button_set_label,
    gtk_check_button_set_active, gtk_check_button_get_active,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible,
    g_signal_connect_data,
}
use core { WINDOWS, noOpBool }

class CheckboxSlot {
    pub btn: GtkCheckButton
    pub fixed: GtkFixed
    pub onToggle: fn(bool)
    pub init(btn: GtkCheckButton, fixed: GtkFixed) {
        this.btn = btn
        this.fixed = fixed
        this.onToggle = noOpBool
    }
}

let CHECKBOXES = new Map<i64, CheckboxSlot>()

@extern(C) {
    pub fn linux_checkbox_toggled(btn: i64, data: i64) {
        let key = btn as i64
        if let some(slot) = CHECKBOXES.get(key) {
            let cb = slot.onToggle
            cb(gtk_check_button_get_active(slot.btn) != 0 as i32)
        }
    }
}

@extern(C) {

pub fn createNativeCheckbox(
    windowHandle: i64,
    text: string, checked: bool,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let cb = gtk_check_button_new_with_label(cstrFromString(text))
    gtk_check_button_set_active(cb, if checked { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_size_request(cb as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(cb as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(cb as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(winSlot.fixed, cb as GtkWidget, x, y)

    let _ = g_signal_connect_data(
        cb as GObject,
        cstrFromString("toggled"),
        linux_checkbox_toggled,
        0 as *void, 0 as i64, 0 as u32
    )

    let handle = cb as i64
    CHECKBOXES[handle] = new CheckboxSlot(cb, winSlot.fixed)
    handle
}

pub fn setOnCheckboxToggle(handle: i64, cb: fn(bool)) {
    if let some(slot) = CHECKBOXES.get(handle) { slot.onToggle = cb }
}
pub fn setCheckboxText(handle: i64, text: string) {
    if let some(slot) = CHECKBOXES.get(handle) {
        gtk_check_button_set_label(slot.btn, cstrFromString(text))
    }
}
pub fn setCheckboxChecked(handle: i64, checked: bool) {
    if let some(slot) = CHECKBOXES.get(handle) {
        gtk_check_button_set_active(slot.btn, if checked { 1 as i32 } else { 0 as i32 })
    }
}
pub fn readCheckboxChecked(handle: i64): bool {
    match CHECKBOXES.get(handle) {
        some(slot) { gtk_check_button_get_active(slot.btn) != 0 as i32 }
        none       { false }
    }
}
pub fn setCheckboxFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = CHECKBOXES.get(handle) {
        gtk_widget_set_size_request(slot.btn as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.btn as GtkWidget, x, y)
    }
}
pub fn setCheckboxEnabled(handle: i64, enabled: bool) {
    if let some(slot) = CHECKBOXES.get(handle) {
        gtk_widget_set_sensitive(slot.btn as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setCheckboxVisible(handle: i64, visible: bool) {
    if let some(slot) = CHECKBOXES.get(handle) {
        gtk_widget_set_visible(slot.btn as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    }
}

} // @extern(C)
