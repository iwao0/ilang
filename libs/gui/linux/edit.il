/// GtkEntry — single-line text input.

use std.ffi { cstrFromString, stringFromCstr }
use gtk {
    GtkWidget, GtkEntry, GtkFixed, GtkWindow, GObject,
    GtkEventController, GtkEventControllerFocus,
    gtk_entry_new, gtk_editable_set_text, gtk_editable_get_text,
    gtk_editable_set_editable,
    gtk_entry_set_placeholder_text, gtk_entry_set_max_length,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible,
    gtk_widget_set_tooltip_text,
    gtk_widget_grab_focus, gtk_widget_add_controller,
    gtk_event_controller_focus_new, gtk_event_controller_get_widget,
    gtk_window_get_focus, gtk_window_set_focus,
    g_signal_connect_data,
}
use core { WINDOWS, WindowSlot, noOpText, noOpClose }

class EditSlot {
    pub entry: GtkEntry
    pub fixed: GtkFixed
    pub winHandle: i64
    pub onChange: fn(string)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub init(entry: GtkEntry, fixed: GtkFixed, winHandle: i64) {
        this.entry = entry
        this.fixed = fixed
        this.winHandle = winHandle
        this.onChange = noOpText
        this.onFocus  = noOpClose
        this.onBlur   = noOpClose
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
    /// `GtkEventControllerFocus.enter` /
    /// `GtkEventControllerFocus.leave` carry
    /// `(controller*, gpointer)`. Resolve the slot through the
    /// controller's attached widget, same trick the Button-side
    /// helpers use.
    pub fn linux_entry_focus_enter(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = EDITS.get(w as i64) {
            let cb = slot.onFocus
            cb()
        }
    }
    pub fn linux_entry_focus_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = EDITS.get(w as i64) {
            let cb = slot.onBlur
            cb()
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
    let entry = gtk_entry_new() as GtkEntry
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

    // Focus controller for onFocus / onBlur. GtkEntry routes
    // focus through the same GtkEventControllerFocus mechanism
    // GtkButton does — `enter` / `leave` map to the public Signals.
    let focusCtl = gtk_event_controller_focus_new()
    gtk_widget_add_controller(entry as GtkWidget, focusCtl)
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("enter"),
        linux_entry_focus_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("leave"),
        linux_entry_focus_leave, 0 as *void, 0 as i64, 0 as u32
    )

    let handle = entry as i64
    EDITS[handle] = new EditSlot(entry, winSlot.fixed, windowHandle)
    handle
}

pub fn setOnEditChange(handle: i64, cb: fn(string)) {
    if let some(slot) = EDITS.get(handle) { slot.onChange = cb }
}
pub fn setOnEditFocus(handle: i64, cb: fn()) {
    if let some(slot) = EDITS.get(handle) { slot.onFocus = cb }
}
pub fn setOnEditBlur(handle: i64, cb: fn()) {
    if let some(slot) = EDITS.get(handle) { slot.onBlur = cb }
}
pub fn focusEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        let _ = gtk_widget_grab_focus(slot.entry as GtkWidget)
    }
}
pub fn blurEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        // Only clear when this entry actually has focus, mirroring
        // the cocoa / win32 paths. Walk the parent WindowSlot to
        // find the GtkWindow that owns focus state.
        if let some(winSlot) = WINDOWS.get(slot.winHandle) {
            let cur = gtk_window_get_focus(winSlot.win)
            if cur as i64 == slot.entry as i64 {
                gtk_window_set_focus(winSlot.win, 0 as GtkWidget)
            }
        }
    }
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
pub fn setEditPlaceholder(handle: i64, text: string) {
    if let some(slot) = EDITS.get(handle) {
        // Empty string → NULL clears the placeholder per GTK
        // convention (an empty cstring would still show a stub).
        if text == "" {
            gtk_entry_set_placeholder_text(slot.entry, 0 as *const char)
        } else {
            gtk_entry_set_placeholder_text(slot.entry, cstrFromString(text))
        }
    }
}
pub fn setEditReadOnly(handle: i64, readOnly: bool) {
    if let some(slot) = EDITS.get(handle) {
        gtk_editable_set_editable(
            slot.entry,
            if readOnly { 0 as i32 } else { 1 as i32 }
        )
    }
}
pub fn setEditMaxLength(handle: i64, max: i32) {
    if let some(slot) = EDITS.get(handle) {
        gtk_entry_set_max_length(slot.entry, max)
    }
}
pub fn setEditTooltip(handle: i64, text: string) {
    if let some(slot) = EDITS.get(handle) {
        // Empty string → pass NULL so GTK clears the tooltip
        // outright. Same logic as the Button-side helper.
        if text == "" {
            gtk_widget_set_tooltip_text(slot.entry as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.entry as GtkWidget, cstrFromString(text))
        }
    }
}

} // @extern(C)
