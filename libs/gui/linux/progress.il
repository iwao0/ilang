/// GtkProgressBar wrapper. GTK fraction is `[0.0, 1.0]`, so we
/// normalize against the [min, max] range gui-core hands us.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkProgressBar, GtkFixed, GdkFrameClock,
    gtk_progress_bar_new, gtk_progress_bar_set_fraction,
    gtk_progress_bar_pulse, gtk_progress_bar_set_pulse_step,
    gtk_orientable_set_orientation,
    gtk_widget_add_tick_callback, gtk_widget_remove_tick_callback,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible, gtk_widget_set_tooltip_text,
}
use core { WINDOWS, resolveGtkParent }

class ProgressSlot {
    pub bar:   GtkProgressBar
    pub fixed: GtkFixed
    pub min:   f64
    pub max:   f64
    pub value: f64
    /// Frame-clock tick callback id while indeterminate-pulsing.
    /// `0` = not pulsing. The bar pulses on every frame so the
    /// motion stays smooth under variable refresh rates.
    pub pulseTick: u32
    pub init(bar: GtkProgressBar, fixed: GtkFixed, min: f64, max: f64, value: f64) {
        this.bar = bar
        this.fixed = fixed
        this.min = min
        this.max = max
        this.value = value
        this.pulseTick = 0 as u32
    }
}

let PROGRESS_BARS = new Map<i64, ProgressSlot>()

@extern(C) {

fn applyFraction(slot: ProgressSlot) {
    let span = slot.max - slot.min
    let frac = if span > 0.0 { (slot.value - slot.min) / span } else { 0.0 }
    let clamped = if frac < 0.0 { 0.0 } else { if frac > 1.0 { 1.0 } else { frac } }
    gtk_progress_bar_set_fraction(slot.bar, clamped)
}

pub fn createNativeProgress(
    windowHandle: i64,
    min: f64, max: f64, value: f64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let bar = gtk_progress_bar_new() as GtkProgressBar
    gtk_widget_set_size_request(bar as GtkWidget, width as i32, height as i32)
    gtk_widget_set_visible(bar as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, bar as GtkWidget, x, y)
    let slot = new ProgressSlot(bar, parent.fixed, min, max, value)
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
pub fn setProgressEnabled(handle: i64, enabled: bool) {
    if let some(slot) = PROGRESS_BARS.get(handle) {
        gtk_widget_set_sensitive(
            slot.bar as GtkWidget,
            if enabled { 1 as i32 } else { 0 as i32 }
        )
    }
}
/// Per-frame tick that pulses the progress bar while
/// indeterminate is on. `user_data` carries the slot handle so
/// we can look the bar up without a global lookup table.
pub fn linux_progress_pulse(widget: i64, clock: i64, data: i64): i32 {
    if let some(slot) = PROGRESS_BARS.get(data) {
        gtk_progress_bar_pulse(slot.bar)
    }
    1 as i32   // G_SOURCE_CONTINUE
}

pub fn setProgressIndeterminate(handle: i64, on: bool) {
    if let some(slot) = PROGRESS_BARS.get(handle) {
        if on {
            if slot.pulseTick == 0 as u32 {
                gtk_progress_bar_set_pulse_step(slot.bar, 0.1)
                slot.pulseTick = gtk_widget_add_tick_callback(
                    slot.bar as GtkWidget, linux_progress_pulse,
                    handle as *void, 0 as i64
                )
            }
        } else {
            if slot.pulseTick != 0 as u32 {
                gtk_widget_remove_tick_callback(slot.bar as GtkWidget, slot.pulseTick)
                slot.pulseTick = 0 as u32
                applyFraction(slot)   // restore the determinate bar
            }
        }
    }
}
pub fn setProgressOrientation(handle: i64, vertical: bool) {
    if let some(slot) = PROGRESS_BARS.get(handle) {
        // GtkOrientation: 0 = horizontal, 1 = vertical.
        let o: i32 = if vertical { 1 as i32 } else { 0 as i32 }
        gtk_orientable_set_orientation(slot.bar as GtkWidget, o)
    }
}
pub fn setProgressTooltip(handle: i64, text: string) {
    if let some(slot) = PROGRESS_BARS.get(handle) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.bar as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.bar as GtkWidget, cstrFromString(text))
        }
    }
}

} // @extern(C)
