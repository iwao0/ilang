/// Accordion. `GtkExpander` with a child `GtkFixed` as the
/// content container so multiple user widgets can be placed
/// absolutely inside. `notify::expanded` fires onToggle when the
/// user clicks the disclosure arrow.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkExpander, GtkFixed, GObject,
    gtk_expander_new, gtk_expander_set_label, gtk_expander_set_child,
    gtk_expander_get_expanded, gtk_expander_set_expanded,
    gtk_fixed_new, gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_visible,
    gtk_widget_set_tooltip_text,
    g_signal_connect_data,
}
use core { resolveGtkParent, GtkParent, GTK_PARENTS }

class AccordionSlot {
    pub expander: GtkExpander
    pub fixed:    GtkFixed   // content host
    pub parentFixed: GtkFixed   // where the expander itself lives
    pub width:    f64
    pub headerH:  f64
    pub contentH: f64
    pub onToggle: fn(bool)
    pub init(expander: GtkExpander, fixed: GtkFixed, parentFixed: GtkFixed) {
        this.expander = expander
        this.fixed = fixed
        this.parentFixed = parentFixed
        this.width    = 0.0
        this.headerH  = 28.0
        this.contentH = 120.0
        this.onToggle = accordionNoOp
    }
}

fn accordionNoOp(expanded: bool) {}

let ACCORDIONS = new Map<i64, AccordionSlot>()

@extern(C) {
    pub fn linux_accordion_notify_expanded(self: i64, pspec: i64, data: i64) {
        if let some(slot) = ACCORDIONS.get(self) {
            let isExpanded = gtk_expander_get_expanded(slot.expander) != 0 as i32
            slot.onToggle(isExpanded)
        }
    }
}

@extern(C) {

pub fn createNativeAccordion(
    windowHandle: i64,
    title: string,
    x: f64, y: f64, width: f64,
    contentH: f64, headerH: f64,
    expanded: bool, visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let exp = gtk_expander_new(cstrFromString(title)) as GtkExpander
    gtk_expander_set_expanded(exp, if expanded { 1 as i32 } else { 0 as i32 })

    let contentFixed = gtk_fixed_new() as GtkFixed
    gtk_widget_set_size_request(contentFixed as GtkWidget, width as i32, contentH as i32)
    gtk_expander_set_child(exp, contentFixed as GtkWidget)

    let totalH = if expanded { headerH + contentH } else { headerH }
    gtk_widget_set_size_request(exp as GtkWidget, width as i32, totalH as i32)
    gtk_widget_set_visible(exp as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, exp as GtkWidget, x, y)

    g_signal_connect_data(
        exp as GObject, cstrFromString("notify::expanded"),
        linux_accordion_notify_expanded, 0 as *void, 0 as i64, 0 as u32
    )

    let handle = exp as i64
    let slot = new AccordionSlot(exp, contentFixed, parent.fixed)
    slot.width = width
    slot.headerH = headerH
    slot.contentH = contentH
    ACCORDIONS[handle] = slot

    // Expose the content fixed as the parent target for widgets
    // added via gui_impl.addChild — `nativeChildHandle` returns
    // the content fixed's handle.
    GTK_PARENTS[contentFixed as i64] = new GtkParent(contentFixed, parent.win)
    handle
}

pub fn accordionContentHandle(handle: i64): i64 {
    match ACCORDIONS.get(handle) {
        some(slot) { slot.fixed as i64 }
        none       { -1 as i64 }
    }
}

pub fn setAccordionTitle(handle: i64, title: string) {
    if let some(slot) = ACCORDIONS.get(handle) {
        gtk_expander_set_label(slot.expander, cstrFromString(title))
    }
}

pub fn setAccordionExpanded(handle: i64, expanded: bool) {
    if let some(slot) = ACCORDIONS.get(handle) {
        gtk_expander_set_expanded(slot.expander, if expanded { 1 as i32 } else { 0 as i32 })
    }
}

pub fn readAccordionExpanded(handle: i64): bool {
    match ACCORDIONS.get(handle) {
        some(slot) { gtk_expander_get_expanded(slot.expander) != 0 as i32 }
        none       { false }
    }
}

pub fn setAccordionFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = ACCORDIONS.get(handle) {
        slot.width = width
        gtk_widget_set_size_request(slot.expander as GtkWidget, width as i32, height as i32)
        gtk_widget_set_size_request(slot.fixed as GtkWidget, width as i32, slot.contentH as i32)
        gtk_fixed_move(slot.parentFixed, slot.expander as GtkWidget, x, y)
    }
}

pub fn setAccordionContentHeight(handle: i64, h: f64) {
    if let some(slot) = ACCORDIONS.get(handle) {
        slot.contentH = h
        gtk_widget_set_size_request(slot.fixed as GtkWidget, slot.width as i32, h as i32)
    }
}

pub fn setAccordionHeaderHeight(handle: i64, h: f64) {
    if let some(slot) = ACCORDIONS.get(handle) {
        slot.headerH = h
        // GtkExpander uses the theme's natural header size; the
        // requested headerH only affects total-size accounting.
    }
}

pub fn setAccordionVisible(handle: i64, visible: bool) {
    if let some(slot) = ACCORDIONS.get(handle) {
        gtk_widget_set_visible(slot.expander as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 })
    }
}

pub fn setAccordionTooltip(handle: i64, text: string) {
    if let some(slot) = ACCORDIONS.get(handle) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.expander as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.expander as GtkWidget, cstrFromString(text))
        }
    }
}

pub fn setOnAccordionToggle(handle: i64, cb: fn(bool)) {
    if let some(slot) = ACCORDIONS.get(handle) { slot.onToggle = cb }
}

} // @extern(C)
