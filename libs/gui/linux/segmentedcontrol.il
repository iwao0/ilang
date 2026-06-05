/// SegmentedControl. Backed by a horizontal `GtkBox` populated
/// with `GtkToggleButton`s, all joined to the first button's group
/// via `gtk_toggle_button_set_group` so exactly one stays active.
/// The `linked` CSS class on the box collapses adjacent button
/// borders into the segmented look.
///
/// The slot is keyed in `SEGMENTEDS` by the `GtkBox` handle (the
/// value exposed to gui-core). Each child button registers itself
/// in `BUTTON_TO_BOX` so the per-button "toggled" callback can
/// route back to the owning slot.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkBox, GtkButton, GtkToggleButton, GtkFixed, GtkOrientation, GObject,
    gtk_box_new, gtk_box_append, gtk_box_set_homogeneous, gtk_box_set_spacing,
    gtk_toggle_button_new_with_label, gtk_toggle_button_set_group,
    gtk_toggle_button_set_active, gtk_toggle_button_get_active,
    gtk_button_set_label,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible, gtk_widget_set_tooltip_text,
    gtk_widget_add_css_class,
    g_signal_connect_data,
}
use core { resolveGtkParent }

class SegmentedSlot {
    pub box:      GtkBox
    pub fixed:    GtkFixed
    pub winHandle: i64
    pub buttons:  GtkToggleButton[]
    pub titles:   string[]
    pub onChange: fn(i32)
    pub init(box: GtkBox, fixed: GtkFixed, winHandle: i64) {
        this.box = box
        this.fixed = fixed
        this.winHandle = winHandle
        this.buttons = []
        this.titles  = []
        this.onChange = segmentedNoOp
    }
}

fn segmentedNoOp(i: i32) {}

let SEGMENTEDS = new Map<i64, SegmentedSlot>()
let BUTTON_TO_BOX = new Map<i64, i64>()

fn findButtonIndex(slot: SegmentedSlot, btn: i64): i32 {
    let i = 0 as i64
    let n = slot.buttons.length
    while i < n {
        if slot.buttons[i] as i64 == btn {
            return i as i32
        }
        i = i + 1
    }
    return -1 as i32
}

@extern(C) {
    pub fn linux_segmented_toggled(btn: i64, data: i64) {
        // Only react to the button that just became active —
        // GtkToggleButton fires "toggled" both for the segment
        // turning ON and (in a grouped pair) the one turning OFF.
        if gtk_toggle_button_get_active(btn as GtkToggleButton) == 0 as i32 {
            return
        }
        if let some(boxHandle) = BUTTON_TO_BOX.get(btn) {
            if let some(slot) = SEGMENTEDS.get(boxHandle) {
                let idx = findButtonIndex(slot, btn)
                slot.onChange(idx)
            }
        }
    }
}

@extern(C) {

pub fn createNativeSegmented(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let box = gtk_box_new(GtkOrientation.horizontal, 0 as i32) as GtkBox
    gtk_box_set_homogeneous(box, 1 as i32)
    gtk_widget_add_css_class(box as GtkWidget, cstrFromString("linked"))
    gtk_widget_set_size_request(box as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(box as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(box as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, box as GtkWidget, x, y)

    let handle = box as i64
    SEGMENTEDS[handle] = new SegmentedSlot(box, parent.fixed, windowHandle)
    handle
}

pub fn segmentedAddSegment(handle: i64, title: string) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        let tb = gtk_toggle_button_new_with_label(cstrFromString(title)) as GtkToggleButton
        // Join the first button's group so only one stays active.
        if slot.buttons.length > 0 as i64 {
            gtk_toggle_button_set_group(tb, slot.buttons[0])
        }
        gtk_box_append(slot.box, tb as GtkWidget)
        slot.buttons.push(tb)
        slot.titles.push(title)
        BUTTON_TO_BOX[tb as i64] = handle
        g_signal_connect_data(
            tb as GObject,
            cstrFromString("toggled"),
            linux_segmented_toggled,
            0 as *void, 0 as i64, 0 as u32
        )
    }
}
pub fn segmentedClearSegments(handle: i64) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        let i = 0 as i64
        let n = slot.buttons.length
        while i < n {
            let btn = slot.buttons[i]
            BUTTON_TO_BOX.delete(btn as i64)
            // GtkBox.remove drops the child from the box; ref count
            // hits zero and the toggle button is freed.
            i = i + 1
        }
        slot.buttons = []
        slot.titles  = []
    }
}
pub fn segmentedSetTitle(handle: i64, index: i32, title: string) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        let i = index as i64
        if i >= 0 as i64 && i < slot.buttons.length {
            slot.titles[i] = title
            gtk_button_set_label(slot.buttons[i] as GtkButton, cstrFromString(title))
        }
    }
}
pub fn segmentedGetTitle(handle: i64, index: i32): string {
    match SEGMENTEDS.get(handle) {
        some(slot) {
            let i = index as i64
            if i >= 0 as i64 && i < slot.titles.length {
                slot.titles[i]
            } else {
                ""
            }
        }
        none { "" }
    }
}
pub fn segmentedGetSelectedIndex(handle: i64): i32 {
    match SEGMENTEDS.get(handle) {
        some(slot) {
            let i = 0 as i64
            let n = slot.buttons.length
            while i < n {
                if gtk_toggle_button_get_active(slot.buttons[i]) != 0 as i32 {
                    return i as i32
                }
                i = i + 1
            }
            return -1 as i32
        }
        none { -1 as i32 }
    }
}
pub fn segmentedSetSelectedIndex(handle: i64, index: i32) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        let i = index as i64
        if i >= 0 as i64 && i < slot.buttons.length {
            gtk_toggle_button_set_active(slot.buttons[i], 1 as i32)
        }
    }
}
pub fn segmentedGetCount(handle: i64): i32 {
    match SEGMENTEDS.get(handle) {
        some(slot) { slot.buttons.length as i32 }
        none       { 0 as i32 }
    }
}
pub fn setSegmentedFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        gtk_widget_set_size_request(slot.box as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.box as GtkWidget, x, y)
    }
}
pub fn setSegmentedEnabled(handle: i64, enabled: bool) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        gtk_widget_set_sensitive(slot.box as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setSegmentedVisible(handle: i64, visible: bool) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        gtk_widget_set_visible(slot.box as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setSegmentedTooltip(handle: i64, text: string) {
    if let some(slot) = SEGMENTEDS.get(handle) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.box as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.box as GtkWidget, cstrFromString(text))
        }
    }
}
pub fn setOnSegmentedChange(handle: i64, cb: fn(i32)) {
    if let some(slot) = SEGMENTEDS.get(handle) { slot.onChange = cb }
}

} // @extern(C)
