/// Numeric entry — `GtkSpinButton`. GTK's spin button is the
/// canonical one-piece numeric input; we just translate the
/// public-side properties onto its setters and connect the
/// `value-changed` signal to our `onChange` callback.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkSpinButton, GtkFixed, GObject,
    GtkEventControllerFocus, GtkEventControllerMotion,
    GtkGestureClick, GtkGestureSingle, GtkEventControllerScroll,
    GtkEventControllerScrollFlags, GtkEventControllerKey,
    GtkEventController,
    gtk_spin_button_new_with_range,
    gtk_spin_button_set_value, gtk_spin_button_get_value,
    gtk_spin_button_set_range, gtk_spin_button_set_increments,
    gtk_spin_button_set_digits, gtk_spin_button_set_wrap,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible, gtk_widget_set_tooltip_text,
    gtk_widget_grab_focus, gtk_widget_add_controller,
    gtk_event_controller_focus_new,
    gtk_event_controller_motion_new,
    gtk_event_controller_scroll_new,
    gtk_event_controller_key_new,
    gtk_event_controller_get_widget,
    gtk_event_controller_get_current_event_state,
    gtk_gesture_click_new,
    gtk_gesture_single_set_button, gtk_gesture_single_get_current_button,
    gdk_keyval_to_lower,
    g_signal_connect_data, g_signal_connect_data2, g_signal_connect_key,
    g_signal_connect_motion, g_signal_connect_click, g_signal_connect_scroll,
}
use window { keyvalToCode }
use core { resolveGtkParent, gdkBtnToW3c, hasMod, noOp1, noOpClickMod, noOpEnterLeave, noOpF64, noOpKey, noOpWheel }

class NumericSlot {
    pub spin:      GtkSpinButton
    pub fixed:     GtkFixed
    pub winHandle: i64
    pub onChange: fn(f64)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub onKeyDown: fn(string, bool, bool, bool, bool)
    pub onKeyUp:   fn(string, bool, bool, bool, bool)
    pub init(spin: GtkSpinButton, fixed: GtkFixed, winHandle: i64) {
        this.spin      = spin
        this.fixed     = fixed
        this.winHandle = winHandle
        this.onChange = noOpF64
        this.onFocus  = noOp1
        this.onBlur   = noOp1
        this.onMouseEnter  = noOpEnterLeave
        this.onMouseLeave  = noOpEnterLeave
        this.onMouseDown   = noOpClickMod
        this.onMouseUp     = noOpClickMod
        this.onContextMenu = noOpClickMod
        this.onMouseWheel  = noOpWheel
        this.onKeyDown = noOpKey
        this.onKeyUp   = noOpKey
    }
}


const NUM_MOD_SHIFT: u32 = 0x00000001 as u32
const NUM_MOD_CTRL:  u32 = 0x00000004 as u32
const NUM_MOD_ALT:   u32 = 0x00000008 as u32
const NUM_MOD_META:  u32 = 0x10000000 as u32

let NUMERICS = new Map<i64, NumericSlot>()

@extern(C) {

pub fn linux_num_value_changed(spin: i64, data: i64) {
    if let some(slot) = NUMERICS.get(spin) {
        let v = gtk_spin_button_get_value(slot.spin)
        slot.onChange(v)
    }
}

pub fn linux_num_focus_enter(ctl: i64, data: i64) {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = NUMERICS.get(w as i64) {
        slot.onFocus()
    }
}
pub fn linux_num_focus_leave(ctl: i64, data: i64) {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = NUMERICS.get(w as i64) {
        slot.onBlur()
    }
}
pub fn linux_num_motion_enter(ctl: i64, x: f64, y: f64, data: i64) {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = NUMERICS.get(w as i64) {
        slot.onMouseEnter(x, y)
    }
}
pub fn linux_num_motion_leave(ctl: i64, data: i64) {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = NUMERICS.get(w as i64) {
        slot.onMouseLeave(0.0, 0.0)
    }
}
pub fn linux_num_pressed(gesture: i64, nPress: i32, x: f64, y: f64, data: i64) {
    let w = gtk_event_controller_get_widget(gesture as GtkEventController)
    if let some(slot) = NUMERICS.get(w as i64) {
        let btn = gtk_gesture_single_get_current_button(gesture as GtkGestureSingle)
        let w3c = gdkBtnToW3c(btn)
        let st = gtk_event_controller_get_current_event_state(
            gesture as GtkEventController
        ) as u32
        let ck = hasMod(st, NUM_MOD_CTRL)
        let sk = hasMod(st, NUM_MOD_SHIFT)
        let ak = hasMod(st, NUM_MOD_ALT)
        let mk = hasMod(st, NUM_MOD_META)
        slot.onMouseDown(x, y, w3c, ck, sk, ak, mk)
        if btn == 3 as u32 {
            let ctx = slot.onContextMenu; ctx(x, y, w3c, ck, sk, ak, mk)
        }
    }
}
pub fn linux_num_released(gesture: i64, nPress: i32, x: f64, y: f64, data: i64) {
    let w = gtk_event_controller_get_widget(gesture as GtkEventController)
    if let some(slot) = NUMERICS.get(w as i64) {
        let btn = gtk_gesture_single_get_current_button(gesture as GtkGestureSingle)
        let st = gtk_event_controller_get_current_event_state(
            gesture as GtkEventController
        ) as u32
        slot.onMouseUp(x, y, gdkBtnToW3c(btn),
           hasMod(st, NUM_MOD_CTRL), hasMod(st, NUM_MOD_SHIFT),
           hasMod(st, NUM_MOD_ALT),  hasMod(st, NUM_MOD_META))
    }
}
pub fn linux_num_scroll(ctl: i64, dx: f64, dy: f64, data: i64): i32 {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = NUMERICS.get(w as i64) {
        slot.onMouseWheel(dx, dy, 0.0, 0.0, false, false, false, false)
    }
    0 as i32
}
pub fn linux_num_key_pressed(
    ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
): i32 {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = NUMERICS.get(w as i64) {
        let code = keyvalToCode(gdk_keyval_to_lower(keyval))
        slot.onKeyDown(code,
           hasMod(state, NUM_MOD_CTRL), hasMod(state, NUM_MOD_SHIFT),
           hasMod(state, NUM_MOD_ALT),  hasMod(state, NUM_MOD_META))
    }
    0 as i32
}
pub fn linux_num_key_released(
    ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
): i32 {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = NUMERICS.get(w as i64) {
        let code = keyvalToCode(gdk_keyval_to_lower(keyval))
        slot.onKeyUp(code,
           hasMod(state, NUM_MOD_CTRL), hasMod(state, NUM_MOD_SHIFT),
           hasMod(state, NUM_MOD_ALT),  hasMod(state, NUM_MOD_META))
    }
    0 as i32
}

pub fn createNativeNumericUpDown(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let spin = gtk_spin_button_new_with_range(0.0, 100.0, 1.0) as GtkSpinButton
    gtk_widget_set_size_request(spin as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(spin as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(spin as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, spin as GtkWidget, x, y)

    g_signal_connect_data(
        spin as GObject, cstrFromString("value-changed"),
        linux_num_value_changed, 0 as *void, 0 as i64, 0 as u32
    )

    let focusCtl = gtk_event_controller_focus_new()
    gtk_widget_add_controller(spin as GtkWidget, focusCtl)
    g_signal_connect_data(focusCtl as GObject, cstrFromString("enter"),
        linux_num_focus_enter, 0 as *void, 0 as i64, 0 as u32)
    g_signal_connect_data(focusCtl as GObject, cstrFromString("leave"),
        linux_num_focus_leave, 0 as *void, 0 as i64, 0 as u32)

    let motionCtl = gtk_event_controller_motion_new() as GtkEventControllerMotion
    gtk_widget_add_controller(spin as GtkWidget, motionCtl as GtkEventController)
    g_signal_connect_motion(motionCtl as GObject, cstrFromString("enter"),
        linux_num_motion_enter, 0 as *void, 0 as i64, 0 as u32)
    g_signal_connect_data(motionCtl as GObject, cstrFromString("leave"),
        linux_num_motion_leave, 0 as *void, 0 as i64, 0 as u32)

    let click = gtk_gesture_click_new() as GtkGestureClick
    gtk_gesture_single_set_button(click as GtkGestureSingle, 0 as u32)
    gtk_widget_add_controller(spin as GtkWidget, click as GtkEventController)
    g_signal_connect_click(click as GObject, cstrFromString("pressed"),
        linux_num_pressed, 0 as *void, 0 as i64, 0 as u32)
    g_signal_connect_click(click as GObject, cstrFromString("released"),
        linux_num_released, 0 as *void, 0 as i64, 0 as u32)

    let scrollCtl = gtk_event_controller_scroll_new(
        GtkEventControllerScrollFlags.vertical | GtkEventControllerScrollFlags.horizontal
    ) as GtkEventControllerScroll
    gtk_widget_add_controller(spin as GtkWidget, scrollCtl as GtkEventController)
    g_signal_connect_scroll(scrollCtl as GObject, cstrFromString("scroll"),
        linux_num_scroll, 0 as *void, 0 as i64, 0 as u32)

    let keyCtl = gtk_event_controller_key_new() as GtkEventControllerKey
    gtk_widget_add_controller(spin as GtkWidget, keyCtl as GtkEventController)
    g_signal_connect_key(keyCtl as GObject, cstrFromString("key-pressed"),
        linux_num_key_pressed, 0 as *void, 0 as i64, 0 as u32)
    g_signal_connect_key(keyCtl as GObject, cstrFromString("key-released"),
        linux_num_key_released, 0 as *void, 0 as i64, 0 as u32)

    let handle = spin as i64
    NUMERICS.set(handle, new NumericSlot(spin, parent.fixed, windowHandle))
    handle
}

pub fn setNumericValue(host: i64, value: f64) {
    if let some(slot) = NUMERICS.get(host) {
        gtk_spin_button_set_value(slot.spin, value)
    }
}

pub fn readNumericValue(host: i64): f64 {
    match NUMERICS.get(host) {
        some(slot) { gtk_spin_button_get_value(slot.spin) }
        none       { 0.0 }
    }
}

pub fn setNumericRange(host: i64, minV: f64, maxV: f64) {
    if let some(slot) = NUMERICS.get(host) {
        gtk_spin_button_set_range(slot.spin, minV, maxV)
    }
}

pub fn setNumericStep(host: i64, step: f64) {
    if let some(slot) = NUMERICS.get(host) {
        // page-step defaults to 10x the unit step; lets PageUp /
        // PageDown move faster without an extra public knob.
        gtk_spin_button_set_increments(slot.spin, step, step * 10.0)
    }
}

pub fn setNumericDecimalPlaces(host: i64, n: i32) {
    if let some(slot) = NUMERICS.get(host) {
        let d: u32 = if n < 0 as i32 { 0 as u32 } else { n as u32 }
        gtk_spin_button_set_digits(slot.spin, d)
    }
}

pub fn setNumericWrap(host: i64, wrap: bool) {
    if let some(slot) = NUMERICS.get(host) {
        gtk_spin_button_set_wrap(slot.spin, if wrap { 1 as i32 } else { 0 as i32 })
    }
}

pub fn setNumericFrame(host: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = NUMERICS.get(host) {
        gtk_widget_set_size_request(slot.spin as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.spin as GtkWidget, x, y)
    }
}

pub fn setNumericEnabled(host: i64, enabled: bool) {
    if let some(slot) = NUMERICS.get(host) {
        gtk_widget_set_sensitive(
            slot.spin as GtkWidget,
            if enabled { 1 as i32 } else { 0 as i32 }
        )
    }
}

pub fn setNumericVisible(host: i64, visible: bool) {
    if let some(slot) = NUMERICS.get(host) {
        gtk_widget_set_visible(
            slot.spin as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}

pub fn setNumericTooltip(host: i64, text: string) {
    if let some(slot) = NUMERICS.get(host) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.spin as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.spin as GtkWidget, cstrFromString(text))
        }
    }
}

pub fn setOnNumericChange(host: i64, cb: fn(f64)) {
    if let some(slot) = NUMERICS.get(host) { slot.onChange = cb }
}
pub fn setOnNumericFocus(host: i64, cb: fn()) {
    if let some(slot) = NUMERICS.get(host) { slot.onFocus = cb }
}
pub fn setOnNumericBlur(host: i64, cb: fn()) {
    if let some(slot) = NUMERICS.get(host) { slot.onBlur = cb }
}
pub fn setOnNumericMouseEnter(host: i64, cb: fn(f64, f64)) {
    if let some(slot) = NUMERICS.get(host) { slot.onMouseEnter = cb }
}
pub fn setOnNumericMouseLeave(host: i64, cb: fn(f64, f64)) {
    if let some(slot) = NUMERICS.get(host) { slot.onMouseLeave = cb }
}
pub fn setOnNumericMouseDown(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = NUMERICS.get(host) { slot.onMouseDown = cb }
}
pub fn setOnNumericMouseUp(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = NUMERICS.get(host) { slot.onMouseUp = cb }
}
pub fn setOnNumericContextMenu(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = NUMERICS.get(host) { slot.onContextMenu = cb }
}
pub fn setOnNumericMouseWheel(
    host: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = NUMERICS.get(host) { slot.onMouseWheel = cb }
}
pub fn setOnNumericKeyDown(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = NUMERICS.get(host) { slot.onKeyDown = cb }
}
pub fn setOnNumericKeyUp(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = NUMERICS.get(host) { slot.onKeyUp = cb }
}

pub fn focusNumeric(host: i64) {
    if let some(slot) = NUMERICS.get(host) {
        let _ = gtk_widget_grab_focus(slot.spin as GtkWidget)
    }
}
pub fn blurNumeric(host: i64) {
    let _ = host
}

} // @extern(C)
