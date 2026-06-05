/// Toolbar — GTK 4 backend. Lays out a horizontal `GtkBox` of
/// `GtkButton`s / `GtkSeparator`s; flexible-space items map to
/// empty `GtkBox`-children with `hexpand` on so GTK splits the
/// leftover width across them automatically.
///
/// The toolbar parks itself at the top of the owning window's
/// inner `GtkFixed`. The window's resize hook
/// (`linux_window_resize`) calls `linux_relayout_toolbar` so the
/// strip tracks the full width of the content area.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkBox, GtkButton, GtkSeparator, GtkOrientation,
    GObject, GtkFixed,
    gtk_box_new, gtk_box_append,
    gtk_button_new_with_label, gtk_button_set_label,
    gtk_separator_new,
    gtk_widget_set_hexpand, gtk_widget_set_size_request,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_get_width, gtk_widget_get_height,
    g_signal_connect_data,
}
use core { WINDOWS }

const TOOLBAR_HEIGHT: f64 = 36.0

pub class ToolbarSlot {
    pub box:         GtkBox
    /// Kind per item (matches the public ToolbarItemKind enum)
    /// — saved so `linux_toolbar_clicked` can route only button
    /// items through the user callback.
    pub kinds:       i32[]
    pub buttons:     GtkButton[]
    pub winHandle:   i64
    pub onItemClick: fn(i32)
    pub init(box: GtkBox, kinds: i32[], buttons: GtkButton[], winHandle: i64) {
        this.box         = box
        this.kinds       = kinds
        this.buttons     = buttons
        this.winHandle   = winHandle
        this.onItemClick = toolbarNoOp
    }
}

fn toolbarNoOp(idx: i32) {}

pub let TOOLBARS = new Map<i64, ToolbarSlot>()
pub let WINDOW_TOOLBAR = new Map<i64, i64>()

/// Reverse index for button-click callbacks: GtkButton pointer
/// → (toolbar handle, item index).
pub class ToolbarBtnInfo {
    pub toolbarHandle: i64
    pub index:         i32
    pub init(t: i64, i: i32) {
        this.toolbarHandle = t
        this.index         = i
    }
}
pub let TOOLBAR_BUTTONS = new Map<i64, ToolbarBtnInfo>()

@extern(C) {

pub fn linux_toolbar_clicked(btn: i64, data: i64) {
    if let some(info) = TOOLBAR_BUTTONS.get(btn) {
        if let some(slot) = TOOLBARS.get(info.toolbarHandle) {
            slot.onItemClick(info.index)
        }
    }
}

pub fn createNativeToolbar(
    windowHandle: i64, kinds: i32[], texts: string[]
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let box = gtk_box_new(GtkOrientation.horizontal, 6 as i32) as GtkBox
    gtk_widget_set_size_request(
        box as GtkWidget, -1 as i32, TOOLBAR_HEIGHT as i32
    )

    let buttons: GtkButton[] = []
    let kindsCopy: i32[] = []
    let n = kinds.length
    let handle = box as i64

    let i = 0 as i64
    while i < n {
        let kind = kinds[i]
        kindsCopy.push(kind)
        if kind == 0 as i32 {
            // Button
            let btn = gtk_button_new_with_label(cstrFromString(texts[i])) as GtkButton
            gtk_box_append(box, btn as GtkWidget)
            TOOLBAR_BUTTONS[btn as i64] = new ToolbarBtnInfo(handle, i as i32)
            g_signal_connect_data(
                btn as GObject, cstrFromString("clicked"),
                linux_toolbar_clicked, 0 as *void, 0 as i64, 0 as u32
            )
            buttons.push(btn)
        } elif kind == 1 as i32 {
            // Separator
            let sep = gtk_separator_new(GtkOrientation.vertical)
            gtk_box_append(box, sep)
            // Push a placeholder so `buttons[i]` aligns with the
            // public item index for setText lookups (which only
            // touch button kinds anyway).
            buttons.push(0 as i64 as GtkButton)
        } else {
            // Flexible space — empty box child with hexpand.
            let filler = gtk_box_new(GtkOrientation.horizontal, 0 as i32)
            gtk_widget_set_hexpand(filler as GtkWidget, 1 as i32)
            gtk_box_append(box, filler as GtkWidget)
            buttons.push(0 as i64 as GtkButton)
        }
        i = i + 1
    }

    // Initial placement: top-left of the window's current content
    // area. linux_relayout_toolbar runs on subsequent resizes.
    let w = gtk_widget_get_width(winSlot.scroll as GtkWidget)
    gtk_fixed_put(winSlot.fixed, box as GtkWidget, 0.0, 0.0)
    if w > 0 as i32 {
        gtk_widget_set_size_request(
            box as GtkWidget, w, TOOLBAR_HEIGHT as i32
        )
    }

    TOOLBARS[handle] = new ToolbarSlot(box, kindsCopy, buttons, windowHandle)
    WINDOW_TOOLBAR[windowHandle] = handle
    handle
}

pub fn setToolbarItemTitle(handle: i64, index: i32, text: string) {
    if let some(slot) = TOOLBARS.get(handle) {
        let i = index as i64
        if i < 0 as i64 || i >= slot.kinds.length { return }
        if slot.kinds[i] != 0 as i32 { return }
        gtk_button_set_label(slot.buttons[i], cstrFromString(text))
    }
}

pub fn setOnToolbarItemClick(handle: i64, cb: fn(i32)) {
    if let some(slot) = TOOLBARS.get(handle) {
        slot.onItemClick = cb
    }
}

/// Called from `linux_window_resize` so the toolbar's GtkBox
/// keeps the full width of the content area.
pub fn linux_relayout_toolbar(windowHandle: i64, w: i32, h: i32) {
    if let some(barHandle) = WINDOW_TOOLBAR.get(windowHandle) {
        if let some(slot) = TOOLBARS.get(barHandle) {
            gtk_widget_set_size_request(
                slot.box as GtkWidget, w, TOOLBAR_HEIGHT as i32
            )
        }
    }
}

} // @extern(C)
