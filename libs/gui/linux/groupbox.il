/// GtkFrame wrapper — the titled-container equivalent of NSBox /
/// the Win32 GROUPBOX style. libs/gui treats it as a positional
/// widget rather than a layout container, so we don't surface
/// `set_child` here; user code adds widgets to the window's main
/// GtkFixed and lets the Frame sit behind them.

use gtk {
    GtkWidget, GtkFrame, GtkFixed,
    gtk_frame_new, gtk_frame_set_label,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_visible,
}
use core { WINDOWS }

class GroupBoxSlot {
    pub frame: GtkFrame
    pub fixed: GtkFixed
    pub init(frame: GtkFrame, fixed: GtkFixed) {
        this.frame = frame
        this.fixed = fixed
    }
}

let GROUPBOXES = new Map<i64, GroupBoxSlot>()

@extern(C) {

pub fn createNativeGroupBox(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let f = gtk_frame_new(cstrFromString(text))
    gtk_widget_set_size_request(f as GtkWidget, width as i32, height as i32)
    gtk_widget_set_visible(f as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(winSlot.fixed, f as GtkWidget, x, y)
    let handle = f as i64
    GROUPBOXES[handle] = new GroupBoxSlot(f, winSlot.fixed)
    handle
}

pub fn setGroupBoxText(handle: i64, text: string) {
    if let some(slot) = GROUPBOXES.get(handle) {
        gtk_frame_set_label(slot.frame, cstrFromString(text))
    }
}

pub fn setGroupBoxFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = GROUPBOXES.get(handle) {
        gtk_widget_set_size_request(slot.frame as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.frame as GtkWidget, x, y)
    }
}

pub fn setGroupBoxVisible(handle: i64, visible: bool) {
    if let some(slot) = GROUPBOXES.get(handle) {
        gtk_widget_set_visible(
            slot.frame as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}

} // @extern(C)
