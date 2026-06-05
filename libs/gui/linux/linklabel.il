/// LinkLabel — `GtkLinkButton`. URI launching is the widget's
/// default `activate-link` behaviour (it hands the URI off to
/// `g_app_info_launch_default_for_uri`); we just listen on
/// `clicked` to fire `onClick`.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkLinkButton, GtkButton, GtkFixed, GObject,
    gtk_link_button_new_with_label, gtk_link_button_set_uri,
    gtk_button_set_label,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible, gtk_widget_set_tooltip_text,
    g_signal_connect_data,
}
use core { resolveGtkParent }

class LinkLabelSlot {
    pub btn:   GtkLinkButton
    pub fixed: GtkFixed
    pub onClick: fn(f64, f64)
    pub init(btn: GtkLinkButton, fixed: GtkFixed) {
        this.btn   = btn
        this.fixed = fixed
        this.onClick = linkLabelNoOp
    }
}

fn linkLabelNoOp(x: f64, y: f64) {}

let LINKLABELS = new Map<i64, LinkLabelSlot>()

@extern(C) {
    pub fn linux_linklabel_clicked(btn: i64, data: i64) {
        if let some(slot) = LINKLABELS.get(btn) {
            let cb = slot.onClick
            cb(0.0, 0.0)
        }
    }
}

@extern(C) {

pub fn createNativeLinkLabel(
    windowHandle: i64,
    text: string, url: string,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    // `gtk_link_button_new_with_label` requires a non-NULL URI;
    // empty string is OK and disables the default open via the
    // activate-link signal returning FALSE on no-op.
    let btn = gtk_link_button_new_with_label(
        cstrFromString(url), cstrFromString(text)
    ) as GtkLinkButton
    gtk_widget_set_size_request(btn as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(btn as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(btn as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, btn as GtkWidget, x, y)

    g_signal_connect_data(
        btn as GObject,
        cstrFromString("clicked"),
        linux_linklabel_clicked,
        0 as *void, 0 as i64, 0 as u32
    )

    let handle = btn as i64
    LINKLABELS[handle] = new LinkLabelSlot(btn, parent.fixed)
    handle
}

pub fn setLinkLabelText(handle: i64, text: string) {
    if let some(slot) = LINKLABELS.get(handle) {
        gtk_button_set_label(slot.btn as GtkButton, cstrFromString(text))
    }
}
pub fn setLinkLabelUrl(handle: i64, url: string) {
    if let some(slot) = LINKLABELS.get(handle) {
        gtk_link_button_set_uri(slot.btn, cstrFromString(url))
    }
}
pub fn setLinkLabelFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = LINKLABELS.get(handle) {
        gtk_widget_set_size_request(slot.btn as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.btn as GtkWidget, x, y)
    }
}
pub fn setLinkLabelEnabled(handle: i64, enabled: bool) {
    if let some(slot) = LINKLABELS.get(handle) {
        gtk_widget_set_sensitive(slot.btn as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setLinkLabelVisible(handle: i64, visible: bool) {
    if let some(slot) = LINKLABELS.get(handle) {
        gtk_widget_set_visible(slot.btn as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setLinkLabelTooltip(handle: i64, text: string) {
    if let some(slot) = LINKLABELS.get(handle) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.btn as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.btn as GtkWidget, cstrFromString(text))
        }
    }
}
pub fn setOnLinkLabelClick(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = LINKLABELS.get(handle) { slot.onClick = cb }
}

} // @extern(C)
