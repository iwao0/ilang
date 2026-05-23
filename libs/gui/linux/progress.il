/// GtkProgressBar wrapper. GTK fraction is `[0.0, 1.0]`, so we
/// normalize against the [min, max] range gui-core hands us.

use gtk {
    GtkWidget, GtkProgressBar, GtkFixed,
    gtk_progress_bar_new, gtk_progress_bar_set_fraction,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_visible,
}
use core { WINDOWS }

class ProgressSlot {
    pub bar:   GtkProgressBar
    pub fixed: GtkFixed
    pub min:   f64
    pub max:   f64
    pub value: f64
    pub init(bar: GtkProgressBar, fixed: GtkFixed, min: f64, max: f64, value: f64) {
        this.bar = bar
        this.fixed = fixed
        this.min = min
        this.max = max
        this.value = value
    }
}

let PROGRESS_BARS = new Map<i64, ProgressSlot>()

fn applyFraction(slot: ProgressSlot) {
    let span = slot.max - slot.min
    let frac = if span > 0.0 { (slot.value - slot.min) / span } else { 0.0 }
    let clamped = if frac < 0.0 { 0.0 } else { if frac > 1.0 { 1.0 } else { frac } }
    gtk_progress_bar_set_fraction(slot.bar, clamped)
}

@extern(C) {

pub fn createNativeProgress(
    windowHandle: i64,
    min: f64, max: f64, value: f64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let bar = gtk_progress_bar_new() as GtkProgressBar
    gtk_widget_set_size_request(bar as GtkWidget, width as i32, height as i32)
    gtk_widget_set_visible(bar as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(winSlot.fixed, bar as GtkWidget, x, y)
    let slot = new ProgressSlot(bar, winSlot.fixed, min, max, value)
    applyFraction(slot)
    let handle = bar as i64
    PROGRESS_BARS[handle] = slot
    handle
}

pub fn setProgressRange(handle: i64, min: f64, max: f64) {
    if let some(slot) = PROGRESS_BARS.get(handle) {
        slot.min = min
        slot.max = max
        applyFraction(slot)
    }
}
pub fn setProgressValue(handle: i64, value: f64) {
    if let some(slot) = PROGRESS_BARS.get(handle) {
        slot.value = value
        applyFraction(slot)
    }
}
pub fn setProgressFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = PROGRESS_BARS.get(handle) {
        gtk_widget_set_size_request(slot.bar as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.bar as GtkWidget, x, y)
    }
}
pub fn setProgressVisible(handle: i64, visible: bool) {
    if let some(slot) = PROGRESS_BARS.get(handle) {
        gtk_widget_set_visible(slot.bar as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    }
}

} // @extern(C)
