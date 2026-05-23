/// GtkComboBoxText wrapper. GTK 4.10 deprecated this in favour of
/// GtkDropDown + GtkStringList, but ComboBoxText still ships and
/// matches the dynamic `addItem` / `clearItems` model gui-core
/// expects without forcing us to also bind GtkStringList.

use gtk {
    GtkWidget, GtkComboBoxText, GtkFixed, GObject,
    gtk_combo_box_text_new, gtk_combo_box_text_append_text,
    gtk_combo_box_text_remove_all,
    gtk_combo_box_set_active, gtk_combo_box_get_active,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible,
    g_signal_connect_data,
}
use core { WINDOWS, noOpI32 }

class ComboSlot {
    pub combo: GtkComboBoxText
    pub fixed: GtkFixed
    pub onSelect: fn(i32)
    pub init(combo: GtkComboBoxText, fixed: GtkFixed) {
        this.combo = combo
        this.fixed = fixed
        this.onSelect = noOpI32
    }
}

let COMBOS = new Map<i64, ComboSlot>()

@extern(C) {
    pub fn linux_combo_changed(c: i64, data: i64) {
        let key = c as i64
        if let some(slot) = COMBOS.get(key) {
            let cb = slot.onSelect
            cb(gtk_combo_box_get_active(slot.combo))
        }
    }
}

@extern(C) {

pub fn createNativeCombo(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let c = gtk_combo_box_text_new()
    gtk_widget_set_size_request(c as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(c as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(c as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(winSlot.fixed, c as GtkWidget, x, y)

    let _ = g_signal_connect_data(
        c as GObject,
        cstrFromString("changed"),
        linux_combo_changed,
        0 as *void, 0 as i64, 0 as u32
    )

    let handle = c as i64
    COMBOS[handle] = new ComboSlot(c, winSlot.fixed)
    handle
}

pub fn setOnComboSelect(handle: i64, cb: fn(i32)) {
    if let some(slot) = COMBOS.get(handle) { slot.onSelect = cb }
}
pub fn addComboItem(handle: i64, item: string) {
    if let some(slot) = COMBOS.get(handle) {
        gtk_combo_box_text_append_text(slot.combo, cstrFromString(item))
    }
}
pub fn clearComboItems(handle: i64) {
    if let some(slot) = COMBOS.get(handle) { gtk_combo_box_text_remove_all(slot.combo) }
}
pub fn setComboSelectedIndex(handle: i64, index: i32) {
    if let some(slot) = COMBOS.get(handle) { gtk_combo_box_set_active(slot.combo, index) }
}
pub fn readComboSelectedIndex(handle: i64): i32 {
    match COMBOS.get(handle) {
        some(slot) { gtk_combo_box_get_active(slot.combo) }
        none       { -1 as i32 }
    }
}
pub fn setComboFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = COMBOS.get(handle) {
        gtk_widget_set_size_request(slot.combo as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.combo as GtkWidget, x, y)
    }
}
pub fn setComboEnabled(handle: i64, enabled: bool) {
    if let some(slot) = COMBOS.get(handle) {
        gtk_widget_set_sensitive(slot.combo as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setComboVisible(handle: i64, visible: bool) {
    if let some(slot) = COMBOS.get(handle) {
        gtk_widget_set_visible(slot.combo as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    }
}

} // @extern(C)
