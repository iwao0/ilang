/// Status bar — GTK 4 backend. Renders as a horizontal `GtkBox`
/// of equal-flex `GtkLabel`s, parked at the bottom of the
/// window's inner `GtkFixed`. The window's resize hook
/// (`linux_window_resize` in window.il) calls
/// `linux_relayout_status_bar` so the strip slides with the
/// window's bottom edge.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkBox, GtkLabel, GtkFixed, GtkOrientation,
    gtk_box_new, gtk_box_append,
    gtk_label_new, gtk_label_set_text, gtk_label_set_xalign,
    gtk_widget_set_hexpand, gtk_widget_set_size_request,
    gtk_fixed_put, gtk_fixed_move, gtk_fixed_remove,
    gtk_widget_get_width, gtk_widget_get_height,
}
use core { WINDOWS }

const STATUS_HEIGHT: f64 = 24.0

pub class StatusBarSlot {
    pub box:       GtkBox
    pub labels:    GtkLabel[]
    pub winHandle: i64
    pub init(box: GtkBox, labels: GtkLabel[], winHandle: i64) {
        this.box       = box
        this.labels    = labels
        this.winHandle = winHandle
    }
}

pub let STATUS_BARS = new Map<i64, StatusBarSlot>()
/// Reverse index: window handle → status-bar handle, so the
/// window's resize callback can find the bar without scanning.
pub let WINDOW_STATUS_BAR = new Map<i64, i64>()

@extern(C) {

fn buildLabel(text: string): GtkLabel {
    let label = gtk_label_new(cstrFromString(text)) as GtkLabel
    gtk_label_set_xalign(label, 0.0 as f32)   // left-align
    gtk_widget_set_hexpand(label as GtkWidget, 1 as i32)
    label
}

pub fn createNativeStatusBar(windowHandle: i64, segments: string[]): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let box = gtk_box_new(GtkOrientation.horizontal, 4 as i32) as GtkBox
    gtk_widget_set_size_request(
        box as GtkWidget, -1 as i32, STATUS_HEIGHT as i32
    )

    let labels: GtkLabel[] = []
    let n = segments.length
    let i = 0 as i64
    while i < n {
        let label = buildLabel(segments[i])
        gtk_box_append(box, label as GtkWidget)
        labels.push(label)
        i = i + 1
    }

    // Initial placement: bottom-left of the window's current content
    // area. The window's notify::default-width/height callback
    // re-runs the layout as the window resizes.
    let w = gtk_widget_get_width(winSlot.scroll as GtkWidget)
    let h = gtk_widget_get_height(winSlot.scroll as GtkWidget)
    let y = if h > 0 as i32 { (h as f64) - STATUS_HEIGHT } else { 0.0 }
    gtk_fixed_put(winSlot.fixed, box as GtkWidget, 0.0, y)
    if w > 0 as i32 {
        gtk_widget_set_size_request(
            box as GtkWidget, w, STATUS_HEIGHT as i32
        )
    }

    let handle = box as i64
    STATUS_BARS[handle] = new StatusBarSlot(box, labels, windowHandle)
    WINDOW_STATUS_BAR[windowHandle] = handle
    handle
}

pub fn setStatusBarSegments(handle: i64, segments: string[]) {
    if let some(slot) = STATUS_BARS.get(handle) {
        // Detach previous labels — GTK 4 has no convenience
        // "removeAll" on GtkBox; walk the cached label list.
        let n = slot.labels.length
        let i = 0 as i64
        while i < n {
            gtk_fixed_remove(slot.box as GtkFixed, slot.labels[i] as GtkWidget)
            i = i + 1
        }
        let newLabels: GtkLabel[] = []
        let m = segments.length
        let j = 0 as i64
        while j < m {
            let label = buildLabel(segments[j])
            gtk_box_append(slot.box, label as GtkWidget)
            newLabels.push(label)
            j = j + 1
        }
        slot.labels = newLabels
    }
}

pub fn setStatusBarSegmentText(handle: i64, index: i32, text: string) {
    if let some(slot) = STATUS_BARS.get(handle) {
        let i = index as i64
        if i < 0 as i64 || i >= slot.labels.length { return }
        gtk_label_set_text(slot.labels[i], cstrFromString(text))
    }
}

/// Called from `linux_window_resize` whenever the owning window
/// changes size. Pins the bar to the bottom edge and stretches
/// it to the full width of the content area.
pub fn linux_relayout_status_bar(windowHandle: i64, w: i32, h: i32) {
    if let some(barHandle) = WINDOW_STATUS_BAR.get(windowHandle) {
        if let some(slot) = STATUS_BARS.get(barHandle) {
            if let some(winSlot) = WINDOWS.get(windowHandle) {
                let y = (h as f64) - STATUS_HEIGHT
                gtk_fixed_move(winSlot.fixed, slot.box as GtkWidget, 0.0, y)
                gtk_widget_set_size_request(
                    slot.box as GtkWidget, w, STATUS_HEIGHT as i32
                )
            }
        }
    }
}

} // @extern(C)
