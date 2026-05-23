/// GtkListBox + GtkScrolledWindow wrapper. Each ilang `addListItem`
/// creates a GtkLabel and appends it as a row.

use gtk {
    GtkWidget, GtkLabel, GtkListBox, GtkListBoxRow, GtkScrolledWindow,
    GtkFixed, GObject,
    gtk_label_new,
    gtk_list_box_new, gtk_list_box_append, gtk_list_box_remove,
    gtk_list_box_select_row, gtk_list_box_unselect_all,
    gtk_list_box_get_selected_row, gtk_list_box_get_row_at_index,
    gtk_list_box_row_get_index,
    gtk_list_box_row_new, gtk_list_box_row_set_child,
    gtk_scrolled_window_new, gtk_scrolled_window_set_child,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible,
    g_signal_connect_data2,
}
use core { WINDOWS, noOpI32 }

class ListSlot {
    pub scroll: GtkScrolledWindow
    pub listBox: GtkListBox
    pub fixed:   GtkFixed
    pub onSelect: fn(i32)
    pub init(scroll: GtkScrolledWindow, lb: GtkListBox, fixed: GtkFixed) {
        this.scroll = scroll
        this.listBox = lb
        this.fixed = fixed
        this.onSelect = noOpI32
    }
}

let LISTS = new Map<i64, ListSlot>()

@extern(C) {
    pub fn linux_list_row_selected(lb: i64, row: i64, data: i64) {
        let key = lb as i64
        if let some(slot) = LISTS.get(key) {
            let r = gtk_list_box_get_selected_row(slot.listBox)
            let idx: i32 = if r as i64 == 0 as i64 {
                -1 as i32
            } else {
                gtk_list_box_row_get_index(r)
            }
            let cb = slot.onSelect
            cb(idx)
        }
    }
}

@extern(C) {

pub fn createNativeList(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let scroll = gtk_scrolled_window_new() as GtkScrolledWindow
    let lb     = gtk_list_box_new() as GtkListBox
    gtk_scrolled_window_set_child(scroll, lb as GtkWidget)
    gtk_widget_set_size_request(scroll as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(lb as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(scroll as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(winSlot.fixed, scroll as GtkWidget, x, y)

    g_signal_connect_data2(
        lb as GObject,
        cstrFromString("row-selected"),
        linux_list_row_selected,
        0 as *void, 0 as i64, 0 as u32
    )

    // The GtkListBox is what fires `row-selected`, so the public
    // handle keys off its pointer rather than the ScrolledWindow.
    let handle = lb as i64
    LISTS[handle] = new ListSlot(scroll, lb, winSlot.fixed)
    handle
}

pub fn setOnListSelect(handle: i64, cb: fn(i32)) {
    if let some(slot) = LISTS.get(handle) { slot.onSelect = cb }
}

pub fn addListItem(handle: i64, item: string) {
    if let some(slot) = LISTS.get(handle) {
        let label = gtk_label_new(cstrFromString(item)) as GtkLabel
        let row = gtk_list_box_row_new() as GtkListBoxRow
        gtk_list_box_row_set_child(row, label as GtkWidget)
        gtk_list_box_append(slot.listBox, row as GtkWidget)
    }
}

pub fn clearListItems(handle: i64) {
    if let some(slot) = LISTS.get(handle) {
        // Walk from index 0 — each remove shifts the remaining rows
        // down, so the next "row at 0" is the next surviving row.
        let row = gtk_list_box_get_row_at_index(slot.listBox, 0 as i32)
        while row as i64 != 0 as i64 {
            gtk_list_box_remove(slot.listBox, row as GtkWidget)
            row = gtk_list_box_get_row_at_index(slot.listBox, 0 as i32)
        }
    }
}

pub fn setListSelectedIndex(handle: i64, index: i32) {
    if let some(slot) = LISTS.get(handle) {
        if index < 0 as i32 {
            gtk_list_box_unselect_all(slot.listBox)
        } else {
            let row = gtk_list_box_get_row_at_index(slot.listBox, index)
            if row as i64 != 0 as i64 {
                gtk_list_box_select_row(slot.listBox, row)
            }
        }
    }
}

pub fn readListSelectedIndex(handle: i64): i32 {
    match LISTS.get(handle) {
        some(slot) {
            let r = gtk_list_box_get_selected_row(slot.listBox)
            if r as i64 == 0 as i64 { -1 as i32 } else { gtk_list_box_row_get_index(r) }
        }
        none { -1 as i32 }
    }
}

pub fn setListFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = LISTS.get(handle) {
        gtk_widget_set_size_request(slot.scroll as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.scroll as GtkWidget, x, y)
    }
}

pub fn setListEnabled(handle: i64, enabled: bool) {
    if let some(slot) = LISTS.get(handle) {
        gtk_widget_set_sensitive(
            slot.listBox as GtkWidget,
            if enabled { 1 as i32 } else { 0 as i32 }
        )
    }
}

pub fn setListVisible(handle: i64, visible: bool) {
    if let some(slot) = LISTS.get(handle) {
        gtk_widget_set_visible(
            slot.scroll as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}

} // @extern(C)
