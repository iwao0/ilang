/// GtkScale (horizontal) wrapper. Cross-platform `Slider` always
/// runs horizontally.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkScale, GtkRange, GtkFixed, GObject, GtkOrientation,
    GtkEventController, GtkEventControllerFocus,
    gtk_scale_new_with_range, gtk_range_set_value, gtk_range_get_value,
    gtk_range_set_range,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible, gtk_widget_set_tooltip_text,
    gtk_widget_grab_focus, gtk_widget_add_controller,
    gtk_event_controller_focus_new,
    gtk_event_controller_get_widget,
    gtk_window_get_focus, gtk_window_set_focus,
    g_signal_connect_data,
}
use core { WINDOWS, noOp1, noOpF64 }

class SliderSlot {
    pub scale: GtkScale
    pub fixed: GtkFixed
    pub winHandle: i64
    pub onChange: fn(f64)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub init(scale: GtkScale, fixed: GtkFixed, winHandle: i64) {
        this.scale = scale
        this.fixed = fixed
        this.winHandle = winHandle
        this.onChange = noOpF64
        this.onFocus  = noOp1
        this.onBlur   = noOp1
    }
}

let SLIDERS = new Map<i64, SliderSlot>()

@extern(C) {
    pub fn linux_slider_value_changed(s: i64, data: i64) {
        let key = s as i64
        if let some(slot) = SLIDERS.get(key) {
            let cb = slot.onChange
            cb(gtk_range_get_value(slot.scale as GtkRange))
        }
    }
    pub fn linux_slider_focus_enter(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = SLIDERS.get(w as i64) {
            let cb = slot.onFocus
            cb()
        }
    }
    pub fn linux_slider_focus_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = SLIDERS.get(w as i64) {
            let cb = slot.onBlur
            cb()
        }
    }
}

@extern(C) {

pub fn createNativeSlider(
    windowHandle: i64,
    min: f64, max: f64, value: f64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    // GtkOrientation: 0 = horizontal, 1 = vertical.
    let s = gtk_scale_new_with_range(0 as GtkOrientation, min, max, 1.0) as GtkScale
    gtk_range_set_value(s as GtkRange, value)
    gtk_widget_set_size_request(s as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(s as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(s as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(winSlot.fixed, s as GtkWidget, x, y)

    g_signal_connect_data(
        s as GObject,
        cstrFromString("value-changed"),
        linux_slider_value_changed,
        0 as *void, 0 as i64, 0 as u32
    )

    let focusCtl = gtk_event_controller_focus_new()
    gtk_widget_add_controller(s as GtkWidget, focusCtl)
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("enter"),
        linux_slider_focus_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("leave"),
        linux_slider_focus_leave, 0 as *void, 0 as i64, 0 as u32
    )

    let handle = s as i64
    SLIDERS[handle] = new SliderSlot(s, winSlot.fixed, windowHandle)
    handle
}

pub fn setOnSliderChange(handle: i64, cb: fn(f64)) {
    if let some(slot) = SLIDERS.get(handle) { slot.onChange = cb }
}
pub fn setSliderRange(handle: i64, min: f64, max: f64) {
    if let some(slot) = SLIDERS.get(handle) { gtk_range_set_range(slot.scale as GtkRange, min, max) }
}
pub fn setSliderValue(handle: i64, value: f64) {
    if let some(slot) = SLIDERS.get(handle) { gtk_range_set_value(slot.scale as GtkRange, value) }
}
pub fn readSliderValue(handle: i64): f64 {
    match SLIDERS.get(handle) {
        some(slot) { gtk_range_get_value(slot.scale as GtkRange) }
        none       { 0.0 }
    }
}
pub fn setSliderFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = SLIDERS.get(handle) {
        gtk_widget_set_size_request(slot.scale as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.scale as GtkWidget, x, y)
    }
}
pub fn setSliderEnabled(handle: i64, enabled: bool) {
    if let some(slot) = SLIDERS.get(handle) {
        gtk_widget_set_sensitive(slot.scale as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setSliderVisible(handle: i64, visible: bool) {
    if let some(slot) = SLIDERS.get(handle) {
        gtk_widget_set_visible(slot.scale as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setOnSliderFocus(handle: i64, cb: fn()) {
    if let some(slot) = SLIDERS.get(handle) { slot.onFocus = cb }
}
pub fn setOnSliderBlur(handle: i64, cb: fn()) {
    if let some(slot) = SLIDERS.get(handle) { slot.onBlur = cb }
}
pub fn focusSlider(handle: i64) {
    if let some(slot) = SLIDERS.get(handle) {
        let _ = gtk_widget_grab_focus(slot.scale as GtkWidget)
    }
}
pub fn blurSlider(handle: i64) {
    if let some(slot) = SLIDERS.get(handle) {
        if let some(winSlot) = WINDOWS.get(slot.winHandle) {
            let cur = gtk_window_get_focus(winSlot.win)
            if cur as i64 == slot.scale as i64 {
                gtk_window_set_focus(winSlot.win, 0 as GtkWidget)
            }
        }
    }
}
pub fn setSliderTooltip(handle: i64, text: string) {
    if let some(slot) = SLIDERS.get(handle) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.scale as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.scale as GtkWidget, cstrFromString(text))
        }
    }
}

} // @extern(C)
