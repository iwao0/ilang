/// ColorWell. `GtkColorDialogButton` paired with a
/// `GtkColorDialog` (single-shot modal picker). `notify::rgba`
/// fires once the user commits a new colour, which matches the
/// "fire on commit, not on drag" semantics of the public API.

use std.ffi { cstrFromString, readF32, writeF32 }
use gtk {
    GtkWidget, GtkColorDialog, GtkColorDialogButton, GtkFixed, GdkRGBA, GObject,
    GtkEventController, GtkEventControllerFocus,
    gtk_color_dialog_new,
    gtk_color_dialog_button_new,
    gtk_color_dialog_button_set_rgba, gtk_color_dialog_button_get_rgba,
    gtk_color_dialog_set_with_alpha,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible, gtk_widget_set_tooltip_text,
    gtk_widget_add_controller, gtk_event_controller_focus_new,
    gtk_event_controller_get_widget,
    g_signal_connect_data,
}
use core { resolveGtkParent }

class ColorWellSlot {
    pub btn:   GtkColorDialogButton
    pub fixed: GtkFixed
    pub onChange: fn(u32)
    pub init(btn: GtkColorDialogButton, fixed: GtkFixed) {
        this.btn = btn
        this.fixed = fixed
        this.onChange = colorWellNoOp
    }
}

fn colorWellNoOp(c: u32) {}

let COLORWELLS = new Map<i64, ColorWellSlot>()

fn argbToRgbaBuf(argb: u32): f32[] {
    let mask: u32 = 0xFF as u32
    let a = (((argb >> 24) & mask) as f32) / 255.0 as f32
    let r = (((argb >> 16) & mask) as f32) / 255.0 as f32
    let g = (((argb >>  8) & mask) as f32) / 255.0 as f32
    let b = (( argb        & mask) as f32) / 255.0 as f32
    [r, g, b, a]
}

fn rgbaPtrToArgb(rgba: *const void): u32 {
    let r = readF32(rgba, 0 as i64)
    let g = readF32(rgba, 4 as i64)
    let b = readF32(rgba, 8 as i64)
    let a = readF32(rgba, 12 as i64)
    let ri = (r * 255.0 as f32) as u32
    let gi = (g * 255.0 as f32) as u32
    let bi = (b * 255.0 as f32) as u32
    let ai = (a * 255.0 as f32) as u32
    (ai << 24) | (ri << 16) | (gi << 8) | bi
}

@extern(C) {
    pub fn linux_colorwell_notify_rgba(self: i64, pspec: i64, data: i64) {
        if let some(slot) = COLORWELLS.get(self) {
            let rgba = gtk_color_dialog_button_get_rgba(slot.btn)
            slot.onChange(rgbaPtrToArgb(rgba as *const void))
        }
    }
}

@extern(C) {

pub fn createNativeColorWell(
    windowHandle: i64,
    initial: u32,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let dlg = gtk_color_dialog_new()
    gtk_color_dialog_set_with_alpha(dlg, 1 as i32)
    let btn = gtk_color_dialog_button_new(dlg) as GtkColorDialogButton

    let buf = argbToRgbaBuf(initial)
    gtk_color_dialog_button_set_rgba(btn, buf as *const f32 as GdkRGBA)

    gtk_widget_set_size_request(btn as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(btn as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(btn as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, btn as GtkWidget, x, y)

    g_signal_connect_data(
        btn as GObject, cstrFromString("notify::rgba"),
        linux_colorwell_notify_rgba, 0 as *void, 0 as i64, 0 as u32
    )

    let handle = btn as i64
    COLORWELLS[handle] = new ColorWellSlot(btn, parent.fixed)
    handle
}

pub fn readColorWellColor(handle: i64): u32 {
    match COLORWELLS.get(handle) {
        some(slot) {
            let rgba = gtk_color_dialog_button_get_rgba(slot.btn)
            rgbaPtrToArgb(rgba as *const void)
        }
        none { 0 as u32 }
    }
}
pub fn setColorWellColor(handle: i64, color: u32) {
    if let some(slot) = COLORWELLS.get(handle) {
        let buf = argbToRgbaBuf(color)
        gtk_color_dialog_button_set_rgba(slot.btn, buf as *const f32 as GdkRGBA)
    }
}
pub fn setColorWellFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = COLORWELLS.get(handle) {
        gtk_widget_set_size_request(slot.btn as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.btn as GtkWidget, x, y)
    }
}
pub fn setColorWellEnabled(handle: i64, enabled: bool) {
    if let some(slot) = COLORWELLS.get(handle) {
        gtk_widget_set_sensitive(slot.btn as GtkWidget,
            if enabled { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setColorWellVisible(handle: i64, visible: bool) {
    if let some(slot) = COLORWELLS.get(handle) {
        gtk_widget_set_visible(slot.btn as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setColorWellTooltip(handle: i64, text: string) {
    if let some(slot) = COLORWELLS.get(handle) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.btn as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.btn as GtkWidget, cstrFromString(text))
        }
    }
}
pub fn setOnColorWellChange(handle: i64, cb: fn(u32)) {
    if let some(slot) = COLORWELLS.get(handle) { slot.onChange = cb }
}

} // @extern(C)
