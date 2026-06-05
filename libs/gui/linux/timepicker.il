/// Time-of-day picker — GTK4 has no first-class time widget,
/// so we lay three `GtkSpinButton`s (hour / minute /
/// [optional second]) in a horizontal `GtkBox`. Each spin
/// emits `value-changed`; we coalesce them into a single
/// `onChange` carrying `(hour, minute, second)`.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkBox, GtkOrientation, GtkSpinButton, GtkFixed, GObject,
    GtkEventControllerFocus, GtkEventControllerMotion,
    GtkGestureClick, GtkGestureSingle, GtkEventControllerScroll,
    GtkEventControllerScrollFlags, GtkEventControllerKey,
    GtkEventController,
    gtk_box_new, gtk_box_append,
    gtk_spin_button_new_with_range,
    gtk_spin_button_set_value, gtk_spin_button_get_value,
    gtk_spin_button_set_increments,
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
    g_signal_connect_data,
    g_signal_connect_motion, g_signal_connect_click, g_signal_connect_scroll,
    g_signal_connect_key,
}
use window { keyvalToCode }
use core { resolveGtkParent, gdkBtnToW3c, hasMod, noOp1, noOpClickMod, noOpEnterLeave, noOpKey, noOpWheel }

class TimeSlot {
    pub box:       GtkBox
    pub hourSpin:  GtkSpinButton
    pub minSpin:   GtkSpinButton
    pub secSpin:   GtkSpinButton
    pub showSeconds: bool
    pub fixed:     GtkFixed
    pub winHandle: i64
    pub onChange: fn(i32, i32, i32)
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
    pub init(box: GtkBox, hourSpin: GtkSpinButton,
             minSpin: GtkSpinButton, secSpin: GtkSpinButton,
             showSeconds: bool, fixed: GtkFixed, winHandle: i64) {
        this.box       = box
        this.hourSpin  = hourSpin
        this.minSpin   = minSpin
        this.secSpin   = secSpin
        this.showSeconds = showSeconds
        this.fixed     = fixed
        this.winHandle = winHandle
        this.onChange = tpNoOpHms
        this.onFocus  = noOp1
        this.onBlur   = noOp1
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseDown  = noOpClickMod
        this.onMouseUp    = noOpClickMod
        this.onContextMenu = noOpClickMod
        this.onMouseWheel = noOpWheel
        this.onKeyDown = noOpKey
        this.onKeyUp   = noOpKey
    }
}

fn tpNoOpHms(h: i32, m: i32, s: i32) {}

const TP_MOD_SHIFT: u32 = 0x00000001 as u32
const TP_MOD_CTRL:  u32 = 0x00000004 as u32
const TP_MOD_ALT:   u32 = 0x00000008 as u32
const TP_MOD_META:  u32 = 0x10000000 as u32

let TIME_PICKERS = new Map<i64, TimeSlot>()
/// Any one of the three spin button handles → owning box handle.
let SPIN_TO_BOX: Map<i64, i64> = new Map<i64, i64>()

@extern(C) {

pub fn linux_tp_value_changed(spin: i64, data: i64) {
    if let some(box) = SPIN_TO_BOX.get(spin) {
        if let some(slot) = TIME_PICKERS.get(box) {
            let h = gtk_spin_button_get_value(slot.hourSpin) as i32
            let m = gtk_spin_button_get_value(slot.minSpin) as i32
            let s = gtk_spin_button_get_value(slot.secSpin) as i32
            let cb = slot.onChange
            cb(h, m, s)
        }
    }
}

pub fn linux_tp_focus_enter(ctl: i64, data: i64) {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = TIME_PICKERS.get(w as i64) {
        let cb = slot.onFocus; cb()
    }
}
pub fn linux_tp_focus_leave(ctl: i64, data: i64) {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = TIME_PICKERS.get(w as i64) {
        let cb = slot.onBlur; cb()
    }
}
pub fn linux_tp_motion_enter(ctl: i64, x: f64, y: f64, data: i64) {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = TIME_PICKERS.get(w as i64) {
        let cb = slot.onMouseEnter; cb(x, y)
    }
}
pub fn linux_tp_motion_leave(ctl: i64, data: i64) {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = TIME_PICKERS.get(w as i64) {
        let cb = slot.onMouseLeave; cb(0.0, 0.0)
    }
}
pub fn linux_tp_pressed(gesture: i64, nPress: i32, x: f64, y: f64, data: i64) {
    let w = gtk_event_controller_get_widget(gesture as GtkEventController)
    if let some(slot) = TIME_PICKERS.get(w as i64) {
        let btn = gtk_gesture_single_get_current_button(gesture as GtkGestureSingle)
        let w3c = gdkBtnToW3c(btn)
        let st = gtk_event_controller_get_current_event_state(
            gesture as GtkEventController
        ) as u32
        let cb = slot.onMouseDown
        cb(x, y, w3c,
           hasMod(st, TP_MOD_CTRL), hasMod(st, TP_MOD_SHIFT),
           hasMod(st, TP_MOD_ALT),  hasMod(st, TP_MOD_META))
        if btn == 3 as u32 {
            let ctx = slot.onContextMenu
            ctx(x, y, w3c,
                hasMod(st, TP_MOD_CTRL), hasMod(st, TP_MOD_SHIFT),
                hasMod(st, TP_MOD_ALT),  hasMod(st, TP_MOD_META))
        }
    }
}
pub fn linux_tp_released(gesture: i64, nPress: i32, x: f64, y: f64, data: i64) {
    let w = gtk_event_controller_get_widget(gesture as GtkEventController)
    if let some(slot) = TIME_PICKERS.get(w as i64) {
        let btn = gtk_gesture_single_get_current_button(gesture as GtkGestureSingle)
        let st = gtk_event_controller_get_current_event_state(
            gesture as GtkEventController
        ) as u32
        let cb = slot.onMouseUp
        cb(x, y, gdkBtnToW3c(btn),
           hasMod(st, TP_MOD_CTRL), hasMod(st, TP_MOD_SHIFT),
           hasMod(st, TP_MOD_ALT),  hasMod(st, TP_MOD_META))
    }
}
pub fn linux_tp_scroll(ctl: i64, dx: f64, dy: f64, data: i64): i32 {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = TIME_PICKERS.get(w as i64) {
        let cb = slot.onMouseWheel
        cb(dx, dy, 0.0, 0.0, false, false, false, false)
    }
    0 as i32
}
pub fn linux_tp_key_pressed(
    ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
): i32 {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = TIME_PICKERS.get(w as i64) {
        let code = keyvalToCode(gdk_keyval_to_lower(keyval))
        let cb = slot.onKeyDown
        cb(code,
           hasMod(state, TP_MOD_CTRL), hasMod(state, TP_MOD_SHIFT),
           hasMod(state, TP_MOD_ALT),  hasMod(state, TP_MOD_META))
    }
    0 as i32
}
pub fn linux_tp_key_released(
    ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
): i32 {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = TIME_PICKERS.get(w as i64) {
        let code = keyvalToCode(gdk_keyval_to_lower(keyval))
        let cb = slot.onKeyUp
        cb(code,
           hasMod(state, TP_MOD_CTRL), hasMod(state, TP_MOD_SHIFT),
           hasMod(state, TP_MOD_ALT),  hasMod(state, TP_MOD_META))
    }
    0 as i32
}

pub fn createNativeTimePicker(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool,
    showSeconds: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let box = gtk_box_new(GtkOrientation.horizontal, 4 as i32) as GtkBox
    let hourSpin = gtk_spin_button_new_with_range(0.0, 23.0, 1.0) as GtkSpinButton
    let minSpin  = gtk_spin_button_new_with_range(0.0, 59.0, 1.0) as GtkSpinButton
    let secSpin  = gtk_spin_button_new_with_range(0.0, 59.0, 1.0) as GtkSpinButton
    gtk_box_append(box, hourSpin as GtkWidget)
    gtk_box_append(box, minSpin as GtkWidget)
    if showSeconds {
        gtk_box_append(box, secSpin as GtkWidget)
    }
    gtk_widget_set_size_request(box as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(box as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(box as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, box as GtkWidget, x, y)

    let key = box as i64
    SPIN_TO_BOX.set(hourSpin as i64, key)
    SPIN_TO_BOX.set(minSpin as i64, key)
    SPIN_TO_BOX.set(secSpin as i64, key)

    g_signal_connect_data(hourSpin as GObject, cstrFromString("value-changed"),
        linux_tp_value_changed, 0 as *void, 0 as i64, 0 as u32)
    g_signal_connect_data(minSpin as GObject, cstrFromString("value-changed"),
        linux_tp_value_changed, 0 as *void, 0 as i64, 0 as u32)
    g_signal_connect_data(secSpin as GObject, cstrFromString("value-changed"),
        linux_tp_value_changed, 0 as *void, 0 as i64, 0 as u32)

    let focusCtl = gtk_event_controller_focus_new()
    gtk_widget_add_controller(box as GtkWidget, focusCtl)
    g_signal_connect_data(focusCtl as GObject, cstrFromString("enter"),
        linux_tp_focus_enter, 0 as *void, 0 as i64, 0 as u32)
    g_signal_connect_data(focusCtl as GObject, cstrFromString("leave"),
        linux_tp_focus_leave, 0 as *void, 0 as i64, 0 as u32)

    let motionCtl = gtk_event_controller_motion_new() as GtkEventControllerMotion
    gtk_widget_add_controller(box as GtkWidget, motionCtl as GtkEventController)
    g_signal_connect_motion(motionCtl as GObject, cstrFromString("enter"),
        linux_tp_motion_enter, 0 as *void, 0 as i64, 0 as u32)
    g_signal_connect_data(motionCtl as GObject, cstrFromString("leave"),
        linux_tp_motion_leave, 0 as *void, 0 as i64, 0 as u32)

    let click = gtk_gesture_click_new() as GtkGestureClick
    gtk_gesture_single_set_button(click as GtkGestureSingle, 0 as u32)
    gtk_widget_add_controller(box as GtkWidget, click as GtkEventController)
    g_signal_connect_click(click as GObject, cstrFromString("pressed"),
        linux_tp_pressed, 0 as *void, 0 as i64, 0 as u32)
    g_signal_connect_click(click as GObject, cstrFromString("released"),
        linux_tp_released, 0 as *void, 0 as i64, 0 as u32)

    let scrollCtl = gtk_event_controller_scroll_new(
        GtkEventControllerScrollFlags.vertical | GtkEventControllerScrollFlags.horizontal
    ) as GtkEventControllerScroll
    gtk_widget_add_controller(box as GtkWidget, scrollCtl as GtkEventController)
    g_signal_connect_scroll(scrollCtl as GObject, cstrFromString("scroll"),
        linux_tp_scroll, 0 as *void, 0 as i64, 0 as u32)

    let keyCtl = gtk_event_controller_key_new() as GtkEventControllerKey
    gtk_widget_add_controller(box as GtkWidget, keyCtl as GtkEventController)
    g_signal_connect_key(keyCtl as GObject, cstrFromString("key-pressed"),
        linux_tp_key_pressed, 0 as *void, 0 as i64, 0 as u32)
    g_signal_connect_key(keyCtl as GObject, cstrFromString("key-released"),
        linux_tp_key_released, 0 as *void, 0 as i64, 0 as u32)

    TIME_PICKERS.set(key, new TimeSlot(
        box, hourSpin, minSpin, secSpin, showSeconds, parent.fixed, windowHandle
    ))
    key
}

pub fn setTimePickerValue(host: i64, hour: i32, minute: i32, second: i32) {
    if let some(slot) = TIME_PICKERS.get(host) {
        gtk_spin_button_set_value(slot.hourSpin, hour as f64)
        gtk_spin_button_set_value(slot.minSpin,  minute as f64)
        gtk_spin_button_set_value(slot.secSpin,  second as f64)
    }
}

pub fn readTimePickerHour(host: i64): i32 {
    match TIME_PICKERS.get(host) {
        some(slot) { gtk_spin_button_get_value(slot.hourSpin) as i32 }
        none       { 0 as i32 }
    }
}
pub fn readTimePickerMinute(host: i64): i32 {
    match TIME_PICKERS.get(host) {
        some(slot) { gtk_spin_button_get_value(slot.minSpin) as i32 }
        none       { 0 as i32 }
    }
}
pub fn readTimePickerSecond(host: i64): i32 {
    match TIME_PICKERS.get(host) {
        some(slot) { gtk_spin_button_get_value(slot.secSpin) as i32 }
        none       { 0 as i32 }
    }
}

pub fn setTimePickerFrame(host: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = TIME_PICKERS.get(host) {
        gtk_widget_set_size_request(slot.box as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.box as GtkWidget, x, y)
    }
}
pub fn setTimePickerEnabled(host: i64, enabled: bool) {
    if let some(slot) = TIME_PICKERS.get(host) {
        gtk_widget_set_sensitive(slot.box as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setTimePickerVisible(host: i64, visible: bool) {
    if let some(slot) = TIME_PICKERS.get(host) {
        gtk_widget_set_visible(slot.box as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setTimePickerTooltip(host: i64, text: string) {
    if let some(slot) = TIME_PICKERS.get(host) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.box as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.box as GtkWidget, cstrFromString(text))
        }
    }
}

pub fn setOnTimePickerChange(host: i64, cb: fn(i32, i32, i32)) {
    if let some(slot) = TIME_PICKERS.get(host) { slot.onChange = cb }
}
pub fn setOnTimePickerFocus(host: i64, cb: fn()) {
    if let some(slot) = TIME_PICKERS.get(host) { slot.onFocus = cb }
}
pub fn setOnTimePickerBlur(host: i64, cb: fn()) {
    if let some(slot) = TIME_PICKERS.get(host) { slot.onBlur = cb }
}
pub fn setOnTimePickerMouseEnter(host: i64, cb: fn(f64, f64)) {
    if let some(slot) = TIME_PICKERS.get(host) { slot.onMouseEnter = cb }
}
pub fn setOnTimePickerMouseLeave(host: i64, cb: fn(f64, f64)) {
    if let some(slot) = TIME_PICKERS.get(host) { slot.onMouseLeave = cb }
}
pub fn setOnTimePickerMouseDown(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { if let some(slot) = TIME_PICKERS.get(host) { slot.onMouseDown = cb } }
pub fn setOnTimePickerMouseUp(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { if let some(slot) = TIME_PICKERS.get(host) { slot.onMouseUp = cb } }
pub fn setOnTimePickerContextMenu(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { if let some(slot) = TIME_PICKERS.get(host) { slot.onContextMenu = cb } }
pub fn setOnTimePickerMouseWheel(
    host: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) { if let some(slot) = TIME_PICKERS.get(host) { slot.onMouseWheel = cb } }
pub fn setOnTimePickerKeyDown(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = TIME_PICKERS.get(host) { slot.onKeyDown = cb }
}
pub fn setOnTimePickerKeyUp(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = TIME_PICKERS.get(host) { slot.onKeyUp = cb }
}

pub fn focusTimePicker(host: i64) {
    if let some(slot) = TIME_PICKERS.get(host) {
        let _ = gtk_widget_grab_focus(slot.hourSpin as GtkWidget)
    }
}
pub fn blurTimePicker(host: i64) {
    let _ = host
}

} // @extern(C)
