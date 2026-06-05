/// Tab strip — `GtkNotebook`. Each tab needs a child widget;
/// Round 1 hands an empty `GtkBox` to every page so the strip
/// renders without the caller having to materialise content.

use std.ffi { cstrFromString }
use gtk {
    g_signal_connect_switch_page,
    GtkWidget, GtkNotebook, GtkBox, GtkLabel, GtkFixed, GObject,
    GtkOrientation,
    gtk_notebook_new, gtk_notebook_append_page,
    gtk_notebook_remove_page, gtk_notebook_get_current_page,
    gtk_notebook_set_current_page, gtk_notebook_get_n_pages,
    gtk_notebook_get_nth_page, gtk_notebook_set_tab_label_text,
    gtk_label_new, gtk_box_new,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible, gtk_widget_set_tooltip_text,
    g_signal_connect_data,
}
use core { WINDOWS, resolveGtkParent }

class TabViewSlot {
    pub notebook: GtkNotebook
    pub fixed:    GtkFixed
    pub onChange: fn(i32)
    pub init(notebook: GtkNotebook, fixed: GtkFixed) {
        this.notebook = notebook
        this.fixed    = fixed
        this.onChange = tabNoOpI32
    }
}

fn tabNoOpI32(i: i32) {}

let TAB_VIEWS = new Map<i64, TabViewSlot>()

@extern(C) {
    /// `void(GtkNotebook*, GtkWidget* page, guint page_num, gpointer)`.
    pub fn linux_tabview_switch_page(nb: i64, page: i64, pageNum: u32, data: i64) {
        if let some(slot) = TAB_VIEWS.get(nb) {
            let cb = slot.onChange
            cb(pageNum as i32)
        }
    }
}

@extern(C) {

pub fn createNativeTabView(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let nb = gtk_notebook_new() as GtkNotebook
    gtk_widget_set_size_request(nb as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(nb as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(nb as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, nb as GtkWidget, x, y)

    g_signal_connect_switch_page(
        nb as GObject, cstrFromString("switch-page"),
        linux_tabview_switch_page, 0 as *void, 0 as i64, 0 as u32
    )

    let handle = nb as i64
    TAB_VIEWS[handle] = new TabViewSlot(nb, parent.fixed)
    handle
}

pub fn setTabViewFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = TAB_VIEWS.get(handle) {
        gtk_widget_set_size_request(slot.notebook as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.notebook as GtkWidget, x, y)
    }
}
pub fn setTabViewEnabled(handle: i64, enabled: bool) {
    if let some(slot) = TAB_VIEWS.get(handle) {
        gtk_widget_set_sensitive(slot.notebook as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setTabViewVisible(handle: i64, visible: bool) {
    if let some(slot) = TAB_VIEWS.get(handle) {
        gtk_widget_set_visible(slot.notebook as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setTabViewTooltip(handle: i64, text: string) {
    if let some(slot) = TAB_VIEWS.get(handle) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.notebook as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.notebook as GtkWidget, cstrFromString(text))
        }
    }
}
pub fn setOnTabViewChange(handle: i64, cb: fn(i32)) {
    if let some(slot) = TAB_VIEWS.get(handle) { slot.onChange = cb }
}

pub fn tabViewAddTab(handle: i64, title: string): i64 {
    match TAB_VIEWS.get(handle) {
        some(slot) {
            let label = gtk_label_new(cstrFromString(title))
            let page = gtk_box_new(0 as GtkOrientation, 0 as i32) as GtkBox
            let pageNum = gtk_notebook_append_page(
                slot.notebook, page as GtkWidget, label
            )
            // The page child widget is the stable identifier — we
            // hand its GtkBox pointer back as `TabItem.handle` so
            // later setters can find the page without trusting an
            // index that may shift after removeTab.
            page as i64
        }
        none { 0 as i64 }
    }
}

pub fn tabViewRemoveTab(handle: i64, index: i32) {
    if let some(slot) = TAB_VIEWS.get(handle) {
        gtk_notebook_remove_page(slot.notebook, index)
    }
}

pub fn tabViewGetSelectedIndex(handle: i64): i32 {
    match TAB_VIEWS.get(handle) {
        some(slot) { gtk_notebook_get_current_page(slot.notebook) }
        none       { -1 as i32 }
    }
}
pub fn tabViewSetSelectedIndex(handle: i64, index: i32) {
    if let some(slot) = TAB_VIEWS.get(handle) {
        gtk_notebook_set_current_page(slot.notebook, index)
    }
}
pub fn tabViewGetTabCount(handle: i64): i32 {
    match TAB_VIEWS.get(handle) {
        some(slot) { gtk_notebook_get_n_pages(slot.notebook) }
        none       { 0 as i32 }
    }
}

pub fn tabItemSetTitle(viewHandle: i64, itemHandle: i64, title: string) {
    if let some(slot) = TAB_VIEWS.get(viewHandle) {
        let child = itemHandle as GtkWidget
        gtk_notebook_set_tab_label_text(slot.notebook, child, cstrFromString(title))
    }
}
pub fn tabItemSetEnabled(viewHandle: i64, itemHandle: i64, enabled: bool) {
    let child = itemHandle as GtkWidget
    gtk_widget_set_sensitive(child, if enabled { 1 as i32 } else { 0 as i32 })
}

} // @extern(C)
