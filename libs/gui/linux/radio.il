/// GtkCheckButton with `gtk_check_button_set_group` — GTK 4's
/// modern radio-button form. Same handler shape as Checkbox.
///
/// Radio grouping: the cross-platform Window picks the "leader"
/// button (the first added) as the group anchor. Subsequent
/// radios receive that leader's GtkCheckButton via
/// `gtk_check_button_set_group`, putting them in the same
/// mutually-exclusive set.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkCheckButton, GtkFixed, GObject,
    GtkEventController, GtkEventControllerFocus, GtkEventControllerMotion,
    GtkEventControllerScroll, GtkEventControllerScrollFlags,
    GtkEventControllerKey,
    GtkGestureSingle, GtkGestureClick,
    gtk_check_button_new_with_label, gtk_check_button_set_label,
    gtk_check_button_set_active, gtk_check_button_get_active,
    gtk_check_button_set_group,
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
    gtk_window_get_focus, gtk_window_set_focus,
    g_signal_connect_data, g_signal_connect_key,
    g_signal_connect_motion, g_signal_connect_click, g_signal_connect_scroll,
}
use window { keyvalToCode }
use core { WINDOWS, resolveGtkParent, noOp1, noOpBool }

class RadioSlot {
    pub btn: GtkCheckButton
    pub fixed: GtkFixed
    pub winHandle: i64
    pub onToggle: fn(bool)
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
    pub init(btn: GtkCheckButton, fixed: GtkFixed, winHandle: i64) {
        this.btn = btn
        this.fixed = fixed
        this.winHandle = winHandle
        this.onToggle = noOpBool
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

fn noOpEnterLeave(x: f64, y: f64) {}
fn noOpClickMod(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn noOpWheel(dx: f64, dy: f64, x: f64, y: f64, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn noOpKey(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {}

const MOD_SHIFT: u32 = 0x00000001 as u32
const MOD_CTRL:  u32 = 0x00000004 as u32
const MOD_ALT:   u32 = 0x00000008 as u32
const MOD_META:  u32 = 0x10000000 as u32
fn hasMod(state: u32, mask: u32): bool { (state & mask) != 0 as u32 }
fn gdkBtnToW3c(gdk: u32): i32 {
    match gdk {
        1 as u32 { 0 as i32 }
        2 as u32 { 1 as i32 }
        3 as u32 { 2 as i32 }
        _        { 0 as i32 }
    }
}

let RADIOS = new Map<i64, RadioSlot>()

/// Per-window leader (first radio added to that window). New
/// radios join its group.
let GROUP_LEADERS = new Map<i64, GtkCheckButton>()

@extern(C) {
    pub fn linux_radio_toggled(btn: i64, data: i64) {
        let key = btn as i64
        if let some(slot) = RADIOS.get(key) {
            let cb = slot.onToggle
            cb(gtk_check_button_get_active(slot.btn) != 0 as i32)
        }
    }
    pub fn linux_radio_focus_enter(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = RADIOS.get(w as i64) {
            let cb = slot.onFocus
            cb()
        }
    }
    pub fn linux_radio_focus_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = RADIOS.get(w as i64) {
            let cb = slot.onBlur
            cb()
        }
    }
    pub fn linux_radio_motion_enter(ctl: i64, x: f64, y: f64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = RADIOS.get(w as i64) {
            let cb = slot.onMouseEnter
            cb(x, y)
        }
    }
    pub fn linux_radio_motion_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = RADIOS.get(w as i64) {
            let cb = slot.onMouseLeave
            cb(0.0, 0.0)
        }
    }
    pub fn linux_radio_pressed(
        gesture: i64, nPress: i32, x: f64, y: f64, data: i64
    ) {
        let w = gtk_event_controller_get_widget(gesture as GtkEventController)
        if let some(slot) = RADIOS.get(w as i64) {
            let btn = gtk_gesture_single_get_current_button(gesture as GtkGestureSingle)
            let w3c = gdkBtnToW3c(btn)
            let st = gtk_event_controller_get_current_event_state(
                gesture as GtkEventController
            ) as u32
            let ck = hasMod(st, MOD_CTRL)
            let sk = hasMod(st, MOD_SHIFT)
            let ak = hasMod(st, MOD_ALT)
            let mk = hasMod(st, MOD_META)
            let cb = slot.onMouseDown
            cb(x, y, w3c, ck, sk, ak, mk)
            if btn == 3 as u32 {
                let ctx = slot.onContextMenu
                ctx(x, y, w3c, ck, sk, ak, mk)
            }
        }
    }
    pub fn linux_radio_released(
        gesture: i64, nPress: i32, x: f64, y: f64, data: i64
    ) {
        let w = gtk_event_controller_get_widget(gesture as GtkEventController)
        if let some(slot) = RADIOS.get(w as i64) {
            let btn = gtk_gesture_single_get_current_button(gesture as GtkGestureSingle)
            let st = gtk_event_controller_get_current_event_state(
                gesture as GtkEventController
            ) as u32
            let cb = slot.onMouseUp
            cb(x, y, gdkBtnToW3c(btn),
               hasMod(st, MOD_CTRL), hasMod(st, MOD_SHIFT),
               hasMod(st, MOD_ALT),  hasMod(st, MOD_META))
        }
    }
    pub fn linux_radio_scroll(
        ctl: i64, dx: f64, dy: f64, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = RADIOS.get(w as i64) {
            let cb = slot.onMouseWheel
            cb(dx, dy, 0.0, 0.0, false, false, false, false)
        }
        0 as i32
    }
    pub fn linux_radio_key_pressed(
        ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = RADIOS.get(w as i64) {
            let code = keyvalToCode(gdk_keyval_to_lower(keyval))
            let cb = slot.onKeyDown
            cb(code,
               hasMod(state, MOD_CTRL), hasMod(state, MOD_SHIFT),
               hasMod(state, MOD_ALT),  hasMod(state, MOD_META))
        }
        0 as i32
    }
    pub fn linux_radio_key_released(
        ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = RADIOS.get(w as i64) {
            let code = keyvalToCode(gdk_keyval_to_lower(keyval))
            let cb = slot.onKeyUp
            cb(code,
               hasMod(state, MOD_CTRL), hasMod(state, MOD_SHIFT),
               hasMod(state, MOD_ALT),  hasMod(state, MOD_META))
        }
        0 as i32
    }
}

@extern(C) {

pub fn createNativeRadio(
    windowHandle: i64,
    text: string, checked: bool,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let r = gtk_check_button_new_with_label(cstrFromString(text)) as GtkCheckButton
    if let some(leader) = GROUP_LEADERS.get(windowHandle) {
        gtk_check_button_set_group(r, leader)
    } else {
        GROUP_LEADERS[windowHandle] = r
    }
    gtk_check_button_set_active(r, if checked { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_size_request(r as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(r as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(r as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, r as GtkWidget, x, y)

    g_signal_connect_data(
        r as GObject,
        cstrFromString("toggled"),
        linux_radio_toggled,
        0 as *void, 0 as i64, 0 as u32
    )

    let focusCtl = gtk_event_controller_focus_new()
    gtk_widget_add_controller(r as GtkWidget, focusCtl)
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("enter"),
        linux_radio_focus_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("leave"),
        linux_radio_focus_leave, 0 as *void, 0 as i64, 0 as u32
    )

    let motionCtl = gtk_event_controller_motion_new() as GtkEventControllerMotion
    gtk_widget_add_controller(r as GtkWidget, motionCtl as GtkEventController)
    g_signal_connect_data(
        motionCtl as GObject, cstrFromString("enter"),
        linux_radio_motion_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        motionCtl as GObject, cstrFromString("leave"),
        linux_radio_motion_leave, 0 as *void, 0 as i64, 0 as u32
    )

    let click = gtk_gesture_click_new() as GtkGestureClick
    gtk_gesture_single_set_button(click as GtkGestureSingle, 0 as u32)
    gtk_widget_add_controller(r as GtkWidget, click as GtkEventController)
    g_signal_connect_click(
        click as GObject, cstrFromString("pressed"),
        linux_radio_pressed, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_click(
        click as GObject, cstrFromString("released"),
        linux_radio_released, 0 as *void, 0 as i64, 0 as u32
    )

    let scrollCtl = gtk_event_controller_scroll_new(
        GtkEventControllerScrollFlags.vertical | GtkEventControllerScrollFlags.horizontal
    ) as GtkEventControllerScroll
    gtk_widget_add_controller(r as GtkWidget, scrollCtl as GtkEventController)
    g_signal_connect_scroll(
        scrollCtl as GObject, cstrFromString("scroll"),
        linux_radio_scroll, 0 as *void, 0 as i64, 0 as u32
    )

    let keyCtl = gtk_event_controller_key_new() as GtkEventControllerKey
    gtk_widget_add_controller(r as GtkWidget, keyCtl as GtkEventController)
    g_signal_connect_key(
        keyCtl as GObject, cstrFromString("key-pressed"),
        linux_radio_key_pressed, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_key(
        keyCtl as GObject, cstrFromString("key-released"),
        linux_radio_key_released, 0 as *void, 0 as i64, 0 as u32
    )

    let handle = r as i64
    RADIOS[handle] = new RadioSlot(r, parent.fixed, windowHandle)
    handle
}

pub fn setOnRadioToggle(handle: i64, cb: fn(bool)) {
    if let some(slot) = RADIOS.get(handle) { slot.onToggle = cb }
}
pub fn setRadioText(handle: i64, text: string) {
    if let some(slot) = RADIOS.get(handle) {
        gtk_check_button_set_label(slot.btn, cstrFromString(text))
    }
}
pub fn setRadioChecked(handle: i64, checked: bool) {
    if let some(slot) = RADIOS.get(handle) {
        gtk_check_button_set_active(slot.btn, if checked { 1 as i32 } else { 0 as i32 })
    }
}
pub fn readRadioChecked(handle: i64): bool {
    match RADIOS.get(handle) {
        some(slot) { gtk_check_button_get_active(slot.btn) != 0 as i32 }
        none       { false }
    }
}
pub fn setRadioFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = RADIOS.get(handle) {
        gtk_widget_set_size_request(slot.btn as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.btn as GtkWidget, x, y)
    }
}
pub fn setRadioEnabled(handle: i64, enabled: bool) {
    if let some(slot) = RADIOS.get(handle) {
        gtk_widget_set_sensitive(slot.btn as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setRadioVisible(handle: i64, visible: bool) {
    if let some(slot) = RADIOS.get(handle) {
        gtk_widget_set_visible(slot.btn as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setOnRadioFocus(handle: i64, cb: fn()) {
    if let some(slot) = RADIOS.get(handle) { slot.onFocus = cb }
}
pub fn setOnRadioBlur(handle: i64, cb: fn()) {
    if let some(slot) = RADIOS.get(handle) { slot.onBlur = cb }
}
pub fn focusRadio(handle: i64) {
    if let some(slot) = RADIOS.get(handle) {
        let _ = gtk_widget_grab_focus(slot.btn as GtkWidget)
    }
}
pub fn blurRadio(handle: i64) {
    if let some(slot) = RADIOS.get(handle) {
        if let some(parent) = resolveGtkParent(slot.winHandle) {
            let cur = gtk_window_get_focus(parent.win)
            if cur as i64 == slot.btn as i64 {
                gtk_window_set_focus(parent.win, 0 as GtkWidget)
            }
        }
    }
}
pub fn setOnRadioMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = RADIOS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnRadioMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = RADIOS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnRadioMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = RADIOS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnRadioMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = RADIOS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnRadioContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = RADIOS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnRadioMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = RADIOS.get(handle) { slot.onMouseWheel = cb }
}
pub fn setOnRadioKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = RADIOS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnRadioKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = RADIOS.get(handle) { slot.onKeyUp = cb }
}
pub fn setRadioTooltip(handle: i64, text: string) {
    if let some(slot) = RADIOS.get(handle) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.btn as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.btn as GtkWidget, cstrFromString(text))
        }
    }
}

} // @extern(C)
