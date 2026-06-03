/// GtkCheckButton with `gtk_check_button_set_group` — GTK 4's
/// modern radio-button form. Same handler shape as Checkbox.
///
/// Radio grouping: the cross-platform Window picks the "leader"
/// button (the first added) as the group anchor. Subsequent
/// radios receive that leader's GtkCheckButton via
/// `gtk_check_button_set_group`, putting them in the same
/// mutually-exclusive set.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkCheckButton, GtkFixed, GObject,
    GtkEventController, GtkEventControllerFocus,
    gtk_check_button_new_with_label, gtk_check_button_set_label,
    gtk_check_button_set_active, gtk_check_button_get_active,
    gtk_check_button_set_group,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible, gtk_widget_set_tooltip_text,
    gtk_widget_grab_focus, gtk_widget_add_controller,
    gtk_event_controller_focus_new,
    gtk_event_controller_get_widget,
    gtk_window_get_focus, gtk_window_set_focus,
    g_signal_connect_data,
}
use core { WINDOWS, noOp1, noOpBool }

class RadioSlot {
    pub btn: GtkCheckButton
    pub fixed: GtkFixed
    pub winHandle: i64
    pub onToggle: fn(bool)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub init(btn: GtkCheckButton, fixed: GtkFixed, winHandle: i64) {
        this.btn = btn
        this.fixed = fixed
        this.winHandle = winHandle
        this.onToggle = noOpBool
        this.onFocus  = noOp1
        this.onBlur   = noOp1
    }
}

let RADIOS = new Map<i64, RadioSlot>()

/// Per-window leader (first radio added to that window). New
/// radios join its group.
let GROUP_LEADERS = new Map<i64, GtkCheckButton>()

@extern(C) {
    pub fn linux_radio_toggled(btn: i64, data: i64) {
        let key = btn as i64
        if let some(slot) = RADIOS.get(key) {
            let cb = slot.onToggle
            cb(gtk_check_button_get_active(slot.btn) != 0 as i32)
        }
    }
    pub fn linux_radio_focus_enter(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = RADIOS.get(w as i64) {
            let cb = slot.onFocus
            cb()
        }
    }
    pub fn linux_radio_focus_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = RADIOS.get(w as i64) {
            let cb = slot.onBlur
            cb()
        }
    }
}

@extern(C) {

pub fn createNativeRadio(
    windowHandle: i64,
    text: string, checked: bool,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let r = gtk_check_button_new_with_label(cstrFromString(text)) as GtkCheckButton
    if let some(leader) = GROUP_LEADERS.get(windowHandle) {
        gtk_check_button_set_group(r, leader)
    } else {
        GROUP_LEADERS[windowHandle] = r
    }
    gtk_check_button_set_active(r, if checked { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_size_request(r as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(r as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(r as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(winSlot.fixed, r as GtkWidget, x, y)

    g_signal_connect_data(
        r as GObject,
        cstrFromString("toggled"),
        linux_radio_toggled,
        0 as *void, 0 as i64, 0 as u32
    )

    let focusCtl = gtk_event_controller_focus_new()
    gtk_widget_add_controller(r as GtkWidget, focusCtl)
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("enter"),
        linux_radio_focus_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("leave"),
        linux_radio_focus_leave, 0 as *void, 0 as i64, 0 as u32
    )

    let handle = r as i64
    RADIOS[handle] = new RadioSlot(r, winSlot.fixed, windowHandle)
    handle
}

pub fn setOnRadioToggle(handle: i64, cb: fn(bool)) {
    if let some(slot) = RADIOS.get(handle) { slot.onToggle = cb }
}
pub fn setRadioText(handle: i64, text: string) {
    if let some(slot) = RADIOS.get(handle) {
        gtk_check_button_set_label(slot.btn, cstrFromString(text))
    }
}
pub fn setRadioChecked(handle: i64, checked: bool) {
    if let some(slot) = RADIOS.get(handle) {
        gtk_check_button_set_active(slot.btn, if checked { 1 as i32 } else { 0 as i32 })
    }
}
pub fn readRadioChecked(handle: i64): bool {
    match RADIOS.get(handle) {
        some(slot) { gtk_check_button_get_active(slot.btn) != 0 as i32 }
        none       { false }
    }
}
pub fn setRadioFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = RADIOS.get(handle) {
        gtk_widget_set_size_request(slot.btn as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.btn as GtkWidget, x, y)
    }
}
pub fn setRadioEnabled(handle: i64, enabled: bool) {
    if let some(slot) = RADIOS.get(handle) {
        gtk_widget_set_sensitive(slot.btn as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setRadioVisible(handle: i64, visible: bool) {
    if let some(slot) = RADIOS.get(handle) {
        gtk_widget_set_visible(slot.btn as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setOnRadioFocus(handle: i64, cb: fn()) {
    if let some(slot) = RADIOS.get(handle) { slot.onFocus = cb }
}
pub fn setOnRadioBlur(handle: i64, cb: fn()) {
    if let some(slot) = RADIOS.get(handle) { slot.onBlur = cb }
}
pub fn focusRadio(handle: i64) {
    if let some(slot) = RADIOS.get(handle) {
        let _ = gtk_widget_grab_focus(slot.btn as GtkWidget)
    }
}
pub fn blurRadio(handle: i64) {
    if let some(slot) = RADIOS.get(handle) {
        if let some(winSlot) = WINDOWS.get(slot.winHandle) {
            let cur = gtk_window_get_focus(winSlot.win)
            if cur as i64 == slot.btn as i64 {
                gtk_window_set_focus(winSlot.win, 0 as GtkWidget)
            }
        }
    }
}
pub fn setRadioTooltip(handle: i64, text: string) {
    if let some(slot) = RADIOS.get(handle) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.btn as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.btn as GtkWidget, cstrFromString(text))
        }
    }
}

} // @extern(C)
