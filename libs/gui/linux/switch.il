/// GtkSwitch wrapper — on/off toggle. Subscribes to
/// `notify::active` so both user clicks and programmatic
/// `gtk_switch_set_active` calls fire `onToggle` consistently.

use std.ffi { cstrFromString }
use gtk {
    g_signal_connect_data2,
    GtkWidget, GtkSwitch, GtkFixed, GObject,
    GtkEventController, GtkEventControllerFocus, GtkEventControllerMotion,
    GtkEventControllerScroll, GtkEventControllerScrollFlags,
    GtkEventControllerKey,
    GtkGestureSingle, GtkGestureClick,
    gtk_switch_new, gtk_switch_set_active, gtk_switch_get_active,
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
use core { WINDOWS, resolveGtkParent, noOp1, noOpBool, hasMod, gdkBtnToW3c, noOpEnterLeave, noOpWheel, noOpKey, noOpClickMod }

class SwitchSlot {
    pub sw: GtkSwitch
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
    pub init(sw: GtkSwitch, fixed: GtkFixed, winHandle: i64) {
        this.sw = sw
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


const MOD_SHIFT: u32 = 0x00000001 as u32
const MOD_CTRL:  u32 = 0x00000004 as u32
const MOD_ALT:   u32 = 0x00000008 as u32
const MOD_META:  u32 = 0x10000000 as u32

let SWITCHES = new Map<i64, SwitchSlot>()

@extern(C) {
    /// `notify::active` fires for both user toggles and
    /// programmatic `gtk_switch_set_active` calls — exactly the
    /// "state changed, full stop" semantics `onToggle` wants.
    pub fn linux_switch_notify_active(self: i64, pspec: i64, data: i64) {
        if let some(slot) = SWITCHES.get(self) {
            slot.onToggle(gtk_switch_get_active(slot.sw) != 0 as i32)
        }
    }
    pub fn linux_switch_focus_enter(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = SWITCHES.get(w as i64) {
            slot.onFocus()
        }
    }
    pub fn linux_switch_focus_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = SWITCHES.get(w as i64) {
            slot.onBlur()
        }
    }
    pub fn linux_switch_motion_enter(ctl: i64, x: f64, y: f64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = SWITCHES.get(w as i64) {
            slot.onMouseEnter(x, y)
        }
    }
    pub fn linux_switch_motion_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = SWITCHES.get(w as i64) {
            slot.onMouseLeave(0.0, 0.0)
        }
    }
    pub fn linux_switch_pressed(
        gesture: i64, nPress: i32, x: f64, y: f64, data: i64
    ) {
        let w = gtk_event_controller_get_widget(gesture as GtkEventController)
        if let some(slot) = SWITCHES.get(w as i64) {
            let btn = gtk_gesture_single_get_current_button(gesture as GtkGestureSingle)
            let w3c = gdkBtnToW3c(btn)
            let st = gtk_event_controller_get_current_event_state(
                gesture as GtkEventController
            ) as u32
            let ck = hasMod(st, MOD_CTRL)
            let sk = hasMod(st, MOD_SHIFT)
            let ak = hasMod(st, MOD_ALT)
            let mk = hasMod(st, MOD_META)
            slot.onMouseDown(x, y, w3c, ck, sk, ak, mk)
            if btn == 3 as u32 {
                let ctx = slot.onContextMenu
                ctx(x, y, w3c, ck, sk, ak, mk)
            }
        }
    }
    pub fn linux_switch_released(
        gesture: i64, nPress: i32, x: f64, y: f64, data: i64
    ) {
        let w = gtk_event_controller_get_widget(gesture as GtkEventController)
        if let some(slot) = SWITCHES.get(w as i64) {
            let btn = gtk_gesture_single_get_current_button(gesture as GtkGestureSingle)
            let st = gtk_event_controller_get_current_event_state(
                gesture as GtkEventController
            ) as u32
            slot.onMouseUp(x, y, gdkBtnToW3c(btn),
               hasMod(st, MOD_CTRL), hasMod(st, MOD_SHIFT),
               hasMod(st, MOD_ALT),  hasMod(st, MOD_META))
        }
    }
    pub fn linux_switch_scroll(
        ctl: i64, dx: f64, dy: f64, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = SWITCHES.get(w as i64) {
            slot.onMouseWheel(dx, dy, 0.0, 0.0, false, false, false, false)
        }
        0 as i32
    }
    pub fn linux_switch_key_pressed(
        ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = SWITCHES.get(w as i64) {
            let code = keyvalToCode(gdk_keyval_to_lower(keyval))
            slot.onKeyDown(code,
               hasMod(state, MOD_CTRL), hasMod(state, MOD_SHIFT),
               hasMod(state, MOD_ALT),  hasMod(state, MOD_META))
        }
        0 as i32
    }
    pub fn linux_switch_key_released(
        ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = SWITCHES.get(w as i64) {
            let code = keyvalToCode(gdk_keyval_to_lower(keyval))
            slot.onKeyUp(code,
               hasMod(state, MOD_CTRL), hasMod(state, MOD_SHIFT),
               hasMod(state, MOD_ALT),  hasMod(state, MOD_META))
        }
        0 as i32
    }
}

@extern(C) {

pub fn createNativeSwitch(
    windowHandle: i64,
    checked: bool,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let sw = gtk_switch_new() as GtkSwitch
    gtk_switch_set_active(sw, if checked { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_size_request(sw as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(sw as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(sw as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, sw as GtkWidget, x, y)

    g_signal_connect_data2(
        sw as GObject,
        cstrFromString("notify::active"),
        linux_switch_notify_active,
        0 as *void, 0 as i64, 0 as u32
    )

    let focusCtl = gtk_event_controller_focus_new()
    gtk_widget_add_controller(sw as GtkWidget, focusCtl)
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("enter"),
        linux_switch_focus_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("leave"),
        linux_switch_focus_leave, 0 as *void, 0 as i64, 0 as u32
    )

    let motionCtl = gtk_event_controller_motion_new() as GtkEventControllerMotion
    gtk_widget_add_controller(sw as GtkWidget, motionCtl as GtkEventController)
    g_signal_connect_motion(
        motionCtl as GObject, cstrFromString("enter"),
        linux_switch_motion_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        motionCtl as GObject, cstrFromString("leave"),
        linux_switch_motion_leave, 0 as *void, 0 as i64, 0 as u32
    )

    let click = gtk_gesture_click_new() as GtkGestureClick
    gtk_gesture_single_set_button(click as GtkGestureSingle, 0 as u32)
    gtk_widget_add_controller(sw as GtkWidget, click as GtkEventController)
    g_signal_connect_click(
        click as GObject, cstrFromString("pressed"),
        linux_switch_pressed, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_click(
        click as GObject, cstrFromString("released"),
        linux_switch_released, 0 as *void, 0 as i64, 0 as u32
    )

    let scrollCtl = gtk_event_controller_scroll_new(
        GtkEventControllerScrollFlags.vertical | GtkEventControllerScrollFlags.horizontal
    ) as GtkEventControllerScroll
    gtk_widget_add_controller(sw as GtkWidget, scrollCtl as GtkEventController)
    g_signal_connect_scroll(
        scrollCtl as GObject, cstrFromString("scroll"),
        linux_switch_scroll, 0 as *void, 0 as i64, 0 as u32
    )

    let keyCtl = gtk_event_controller_key_new() as GtkEventControllerKey
    gtk_widget_add_controller(sw as GtkWidget, keyCtl as GtkEventController)
    g_signal_connect_key(
        keyCtl as GObject, cstrFromString("key-pressed"),
        linux_switch_key_pressed, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_key(
        keyCtl as GObject, cstrFromString("key-released"),
        linux_switch_key_released, 0 as *void, 0 as i64, 0 as u32
    )

    let handle = sw as i64
    SWITCHES[handle] = new SwitchSlot(sw, parent.fixed, windowHandle)
    handle
}

pub fn setOnSwitchToggle(handle: i64, cb: fn(bool)) {
    if let some(slot) = SWITCHES.get(handle) { slot.onToggle = cb }
}
pub fn setSwitchChecked(handle: i64, checked: bool) {
    if let some(slot) = SWITCHES.get(handle) {
        gtk_switch_set_active(slot.sw, if checked { 1 as i32 } else { 0 as i32 })
    }
}
pub fn readSwitchChecked(handle: i64): bool {
    match SWITCHES.get(handle) {
        some(slot) { gtk_switch_get_active(slot.sw) != 0 as i32 }
        none       { false }
    }
}
pub fn setSwitchFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = SWITCHES.get(handle) {
        gtk_widget_set_size_request(slot.sw as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.sw as GtkWidget, x, y)
    }
}
pub fn setSwitchEnabled(handle: i64, enabled: bool) {
    if let some(slot) = SWITCHES.get(handle) {
        gtk_widget_set_sensitive(slot.sw as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setSwitchVisible(handle: i64, visible: bool) {
    if let some(slot) = SWITCHES.get(handle) {
        gtk_widget_set_visible(slot.sw as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setOnSwitchFocus(handle: i64, cb: fn()) {
    if let some(slot) = SWITCHES.get(handle) { slot.onFocus = cb }
}
pub fn setOnSwitchBlur(handle: i64, cb: fn()) {
    if let some(slot) = SWITCHES.get(handle) { slot.onBlur = cb }
}
pub fn focusSwitch(handle: i64) {
    if let some(slot) = SWITCHES.get(handle) {
        let _ = gtk_widget_grab_focus(slot.sw as GtkWidget)
    }
}
pub fn blurSwitch(handle: i64) {
    if let some(slot) = SWITCHES.get(handle) {
        if let some(parent) = resolveGtkParent(slot.winHandle) {
            let cur = gtk_window_get_focus(parent.win)
            if cur as i64 == slot.sw as i64 {
                gtk_window_set_focus(parent.win, 0 as GtkWidget)
            }
        }
    }
}
pub fn setOnSwitchMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = SWITCHES.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnSwitchMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = SWITCHES.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnSwitchMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = SWITCHES.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnSwitchMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = SWITCHES.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnSwitchContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = SWITCHES.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnSwitchMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = SWITCHES.get(handle) { slot.onMouseWheel = cb }
}
pub fn setOnSwitchKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = SWITCHES.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnSwitchKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = SWITCHES.get(handle) { slot.onKeyUp = cb }
}
pub fn setSwitchTooltip(handle: i64, text: string) {
    if let some(slot) = SWITCHES.get(handle) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.sw as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.sw as GtkWidget, cstrFromString(text))
        }
    }
}

} // @extern(C)
