/// GtkButton wrapper. Mirrors libs/gui/cocoa/button.il and
/// libs/gui/win32/button.il.
///
/// Click handler dispatch:
/// - GTK 4's `clicked` signal callback shape is
///   `void(*)(GtkButton*, gpointer)`.
/// - One shared C handler (`linux_button_clicked`) resolves the
///   GtkButton pointer back to a ButtonSlot through `BUTTONS`.
/// - g_signal_connect_data takes the handler via `...`.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkButton, GtkFixed, GObject,
    gtk_button_new_with_label, gtk_button_set_label,
    gtk_fixed_put, gtk_fixed_move, gtk_fixed_remove,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible,
    g_signal_connect_data,
}
use core { WINDOWS, noOpClick }

class ButtonSlot {
    pub btn:     GtkButton
    pub fixed:   GtkFixed
    pub x:       f64
    pub y:       f64
    pub width:   f64
    pub height:  f64
    pub onClick: fn(f64, f64, i32)
    pub init(btn: GtkButton, fixed: GtkFixed, x: f64, y: f64, w: f64, h: f64) {
        this.btn = btn
        this.fixed = fixed
        this.x = x
        this.y = y
        this.width = w
        this.height = h
        this.onClick = noOpClick
    }
}

let BUTTONS = new Map<i64, ButtonSlot>()

@extern(C) {
    pub fn linux_button_clicked(btn: i64, data: i64) {
        let key = btn as i64
        if let some(slot) = BUTTONS.get(key) {
            // Cocoa / Win32 surface the button's centre as the click
            // pos when the OS gives us none — match that.
            slot.onClick(
                slot.x + slot.width / 2.0,
                slot.y + slot.height / 2.0,
                0 as i32
            )
        }
    }
}

@extern(C) {

pub fn createNativeButton(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let btn = gtk_button_new_with_label(cstrFromString(text)) as GtkButton
    gtk_widget_set_size_request(btn as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(
        btn as GtkWidget,
        if enabled { 1 as i32 } else { 0 as i32 }
    )
    gtk_widget_set_visible(
        btn as GtkWidget,
        if visible { 1 as i32 } else { 0 as i32 }
    )
    gtk_fixed_put(winSlot.fixed, btn as GtkWidget, x, y)

    g_signal_connect_data(
        btn as GObject,
        cstrFromString("clicked"),
        linux_button_clicked,
        0 as *void,
        0 as i64,
        0 as u32
    )

    let handle = btn as i64
    BUTTONS[handle] = new ButtonSlot(btn, winSlot.fixed, x, y, width, height)
    handle
}

pub fn setOnButtonClick(handle: i64, cb: fn(f64, f64, i32)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onClick = cb }
}

pub fn setButtonText(handle: i64, text: string) {
    if let some(slot) = BUTTONS.get(handle) {
        gtk_button_set_label(slot.btn, cstrFromString(text))
    }
}

pub fn setButtonFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = BUTTONS.get(handle) {
        slot.x = x
        slot.y = y
        slot.width = width
        slot.height = height
        gtk_widget_set_size_request(slot.btn as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.btn as GtkWidget, x, y)
    }
}

pub fn setButtonEnabled(handle: i64, enabled: bool) {
    if let some(slot) = BUTTONS.get(handle) {
        gtk_widget_set_sensitive(
            slot.btn as GtkWidget,
            if enabled { 1 as i32 } else { 0 as i32 }
        )
    }
}

pub fn setButtonVisible(handle: i64, visible: bool) {
    if let some(slot) = BUTTONS.get(handle) {
        gtk_widget_set_visible(
            slot.btn as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}

} // @extern(C)
