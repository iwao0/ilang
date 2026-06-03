/// GtkComboBoxText wrapper. GTK 4.10 deprecated this in favour of
/// GtkDropDown + GtkStringList, but ComboBoxText still ships and
/// matches the dynamic `addItem` / `clearItems` model gui-core
/// expects without forcing us to also bind GtkStringList.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkComboBoxText, GtkFixed, GObject,
    GtkEventController, GtkEventControllerFocus,
    gtk_combo_box_text_new, gtk_combo_box_text_append_text,
    gtk_combo_box_text_remove_all,
    gtk_combo_box_set_active, gtk_combo_box_get_active,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible, gtk_widget_set_tooltip_text,
    gtk_widget_grab_focus, gtk_widget_add_controller,
    gtk_event_controller_focus_new,
    gtk_event_controller_get_widget,
    gtk_window_get_focus, gtk_window_set_focus,
    g_signal_connect_data,
}
use core { WINDOWS, noOp1, noOpI32 }

class ComboSlot {
    pub combo: GtkComboBoxText
    pub fixed: GtkFixed
    pub winHandle: i64
    pub onSelect: fn(i32)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub init(combo: GtkComboBoxText, fixed: GtkFixed, winHandle: i64) {
        this.combo = combo
        this.fixed = fixed
        this.winHandle = winHandle
        this.onSelect = noOpI32
        this.onFocus  = noOp1
        this.onBlur   = noOp1
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
    pub fn linux_combo_focus_enter(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = COMBOS.get(w as i64) {
            let cb = slot.onFocus
            cb()
        }
    }
    pub fn linux_combo_focus_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = COMBOS.get(w as i64) {
            let cb = slot.onBlur
            cb()
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

    g_signal_connect_data(
        c as GObject,
        cstrFromString("changed"),
        linux_combo_changed,
        0 as *void, 0 as i64, 0 as u32
    )

    let focusCtl = gtk_event_controller_focus_new()
    gtk_widget_add_controller(c as GtkWidget, focusCtl)
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("enter"),
        linux_combo_focus_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("leave"),
        linux_combo_focus_leave, 0 as *void, 0 as i64, 0 as u32
    )

    let handle = c as i64
    COMBOS[handle] = new ComboSlot(c, winSlot.fixed, windowHandle)
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
pub fn setOnComboFocus(handle: i64, cb: fn()) {
    if let some(slot) = COMBOS.get(handle) { slot.onFocus = cb }
}
pub fn setOnComboBlur(handle: i64, cb: fn()) {
    if let some(slot) = COMBOS.get(handle) { slot.onBlur = cb }
}
pub fn focusCombo(handle: i64) {
    if let some(slot) = COMBOS.get(handle) {
        let _ = gtk_widget_grab_focus(slot.combo as GtkWidget)
    }
}
pub fn blurCombo(handle: i64) {
    if let some(slot) = COMBOS.get(handle) {
        if let some(winSlot) = WINDOWS.get(slot.winHandle) {
            let cur = gtk_window_get_focus(winSlot.win)
            if cur as i64 == slot.combo as i64 {
                gtk_window_set_focus(winSlot.win, 0 as GtkWidget)
            }
        }
    }
}
pub fn setComboTooltip(handle: i64, text: string) {
    if let some(slot) = COMBOS.get(handle) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.combo as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.combo as GtkWidget, cstrFromString(text))
        }
    }
}

} // @extern(C)
