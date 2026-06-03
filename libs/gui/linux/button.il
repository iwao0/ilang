/// GtkButton wrapper. Mirrors libs/gui/cocoa/button.il and
/// libs/gui/win32/button.il.
///
/// Click handler dispatch:
/// - GTK 4's `clicked` signal callback shape is
///   `void(*)(GtkButton*, gpointer)`.
/// - One shared C handler (`linux_button_clicked`) resolves the
///   GtkButton pointer back to a ButtonSlot through `BUTTONS`.
/// - g_signal_connect_data takes the handler via `...`.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkButton, GtkFixed, GtkWindow, GObject,
    GtkEventController,
    gtk_button_new_with_label, gtk_button_set_label,
    gtk_fixed_put, gtk_fixed_move, gtk_fixed_remove,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible, gtk_widget_activate,
    gtk_widget_set_receives_default, gtk_window_set_default_widget,
    gtk_widget_grab_focus, gtk_widget_add_controller,
    gtk_event_controller_focus_new,
    g_signal_connect_data,
}
use core { WINDOWS, noOpClick, noOpClose }

class ButtonSlot {
    pub btn:     GtkButton
    pub fixed:   GtkFixed
    pub win:     GtkWindow
    pub winHandle: i64
    pub x:       f64
    pub y:       f64
    pub width:   f64
    pub height:  f64
    pub onClick: fn(f64, f64, i32)
    pub onFocus: fn()
    pub onBlur:  fn()
    pub init(
        btn: GtkButton, fixed: GtkFixed, win: GtkWindow, winHandle: i64,
        x: f64, y: f64, w: f64, h: f64
    ) {
        this.btn = btn
        this.fixed = fixed
        this.win = win
        this.winHandle = winHandle
        this.x = x
        this.y = y
        this.width = w
        this.height = h
        this.onClick = noOpClick
        this.onFocus = noOpClose
        this.onBlur  = noOpClose
    }
}

let BUTTONS = new Map<i64, ButtonSlot>()

/// Window handle → current default-button handle. GTK's
/// `set_default_widget` auto-clears the previous default on its
/// own, but tracking the holder lets `setButtonDefault(_, false)`
/// know whether to call it with `NULL`.
let DEFAULT_BUTTONS = new Map<i64, i64>()

@extern(C) {
    pub fn linux_button_clicked(btn: i64, data: i64) {
        let key = btn as i64
        if let some(slot) = BUTTONS.get(key) {
            // Cocoa / Win32 surface the button's centre as the click
            // pos when the OS gives us none — match that.
            slot.onClick(
                slot.x + slot.width / 2.0,
                slot.y + slot.height / 2.0,
                0 as i32
            )
        }
    }
    /// `enter` / `leave` signals on `GtkEventControllerFocus` carry
    /// `(GtkEventControllerFocus*, gpointer)`. The controller knows
    /// which widget it's attached to via
    /// `gtk_event_controller_get_widget`; use that as the BUTTONS
    /// key (same key createNativeButton stored).
    pub fn linux_button_focus_enter(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = BUTTONS.get(w as i64) {
            let cb = slot.onFocus
            cb()
        }
    }
    pub fn linux_button_focus_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = BUTTONS.get(w as i64) {
            let cb = slot.onBlur
            cb()
        }
    }
}

@extern(C) {

pub fn createNativeButton(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool, isDefault: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let btn = gtk_button_new_with_label(cstrFromString(text)) as GtkButton
    gtk_widget_set_size_request(btn as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(
        btn as GtkWidget,
        if enabled { 1 as i32 } else { 0 as i32 }
    )
    gtk_widget_set_visible(
        btn as GtkWidget,
        if visible { 1 as i32 } else { 0 as i32 }
    )
    gtk_fixed_put(winSlot.fixed, btn as GtkWidget, x, y)

    g_signal_connect_data(
        btn as GObject,
        cstrFromString("clicked"),
        linux_button_clicked,
        0 as *void,
        0 as i64,
        0 as u32
    )

    // Attach a focus event controller so the widget's focus /
    // blur transitions surface to our linux_button_focus_*
    // handlers. The controller's `widget` association is set by
    // gtk_widget_add_controller; the handlers look up the
    // ButtonSlot through `gtk_event_controller_get_widget`.
    let focusCtl = gtk_event_controller_focus_new()
    gtk_widget_add_controller(btn as GtkWidget, focusCtl)
    g_signal_connect_data(
        focusCtl as GObject,
        cstrFromString("enter"),
        linux_button_focus_enter,
        0 as *void,
        0 as i64,
        0 as u32
    )
    g_signal_connect_data(
        focusCtl as GObject,
        cstrFromString("leave"),
        linux_button_focus_leave,
        0 as *void,
        0 as i64,
        0 as u32
    )

    let handle = btn as i64
    BUTTONS[handle] = new ButtonSlot(
        btn, winSlot.fixed, winSlot.win, windowHandle,
        x, y, width, height
    )
    if isDefault { setButtonDefault(handle, true) }
    handle
}

pub fn setOnButtonClick(handle: i64, cb: fn(f64, f64, i32)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onClick = cb }
}

pub fn setButtonText(handle: i64, text: string) {
    if let some(slot) = BUTTONS.get(handle) {
        gtk_button_set_label(slot.btn, cstrFromString(text))
    }
}

pub fn setButtonFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = BUTTONS.get(handle) {
        slot.x = x
        slot.y = y
        slot.width = width
        slot.height = height
        gtk_widget_set_size_request(slot.btn as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.btn as GtkWidget, x, y)
    }
}

pub fn setButtonEnabled(handle: i64, enabled: bool) {
    if let some(slot) = BUTTONS.get(handle) {
        gtk_widget_set_sensitive(
            slot.btn as GtkWidget,
            if enabled { 1 as i32 } else { 0 as i32 }
        )
    }
}

pub fn setButtonVisible(handle: i64, visible: bool) {
    if let some(slot) = BUTTONS.get(handle) {
        gtk_widget_set_visible(
            slot.btn as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}
pub fn clickButton(handle: i64) {
    if let some(slot) = BUTTONS.get(handle) {
        // `gtk_widget_activate` fires the "clicked" signal on
        // GtkButton, which our `linux_button_clicked` handler
        // routes back to `onClick`.
        gtk_widget_activate(slot.btn as GtkWidget)
    }
}
pub fn setOnButtonFocus(handle: i64, cb: fn()) {
    if let some(slot) = BUTTONS.get(handle) { slot.onFocus = cb }
}
pub fn setOnButtonBlur(handle: i64, cb: fn()) {
    if let some(slot) = BUTTONS.get(handle) { slot.onBlur = cb }
}
pub fn focusButton(handle: i64) {
    if let some(slot) = BUTTONS.get(handle) {
        // `grab_focus` triggers the focus controller's `enter`
        // signal on this widget (and `leave` on the previous
        // holder), which routes through our linux_button_focus_*
        // handlers back to `onFocus` / `onBlur`.
        let _ = gtk_widget_grab_focus(slot.btn as GtkWidget)
    }
}
pub fn setButtonDefault(handle: i64, isDefault: bool) {
    if let some(slot) = BUTTONS.get(handle) {
        if isDefault {
            gtk_widget_set_receives_default(slot.btn as GtkWidget, 1 as i32)
            gtk_window_set_default_widget(slot.win, slot.btn as GtkWidget)
            DEFAULT_BUTTONS[slot.winHandle] = handle
        } else {
            gtk_widget_set_receives_default(slot.btn as GtkWidget, 0 as i32)
            // Clear only if *this* button is still the window's
            // default; otherwise another button already took over.
            if let some(cur) = DEFAULT_BUTTONS.get(slot.winHandle) {
                if cur == handle {
                    gtk_window_set_default_widget(slot.win, 0 as GtkWidget)
                    DEFAULT_BUTTONS.delete(slot.winHandle)
                }
            }
        }
    }
}

} // @extern(C)
