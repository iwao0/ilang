/// GtkComboBoxText wrapper. GTK 4.10 deprecated this in favour of
/// GtkDropDown + GtkStringList, but ComboBoxText still ships and
/// matches the dynamic `addItem` / `clearItems` model gui-core
/// expects without forcing us to also bind GtkStringList.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkComboBoxText, GtkFixed, GObject,
    GtkEventController, GtkEventControllerFocus, GtkEventControllerMotion,
    GtkEventControllerScroll, GtkEventControllerScrollFlags,
    GtkEventControllerKey,
    GtkGestureSingle, GtkGestureClick,
    gtk_combo_box_text_new, gtk_combo_box_text_append_text,
    gtk_combo_box_text_remove_all,
    gtk_combo_box_set_active, gtk_combo_box_get_active,
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
use core { WINDOWS, noOp1, noOpI32 }

class ComboSlot {
    pub combo: GtkComboBoxText
    pub fixed: GtkFixed
    pub winHandle: i64
    pub onSelect: fn(i32)
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
    pub init(combo: GtkComboBoxText, fixed: GtkFixed, winHandle: i64) {
        this.combo = combo
        this.fixed = fixed
        this.winHandle = winHandle
        this.onSelect = noOpI32
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

let COMBOS = new Map<i64, ComboSlot>()

@extern(C) {
    pub fn linux_combo_changed(c: i64, data: i64) {
        let key = c as i64
        if let some(slot) = COMBOS.get(key) {
            let cb = slot.onSelect
            cb(gtk_combo_box_get_active(slot.combo))
        }
    }
    pub fn linux_combo_focus_enter(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = COMBOS.get(w as i64) {
            let cb = slot.onFocus
            cb()
        }
    }
    pub fn linux_combo_focus_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = COMBOS.get(w as i64) {
            let cb = slot.onBlur
            cb()
        }
    }
    pub fn linux_combo_motion_enter(ctl: i64, x: f64, y: f64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = COMBOS.get(w as i64) {
            let cb = slot.onMouseEnter
            cb(x, y)
        }
    }
    pub fn linux_combo_motion_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = COMBOS.get(w as i64) {
            let cb = slot.onMouseLeave
            cb(0.0, 0.0)
        }
    }
    pub fn linux_combo_pressed(
        gesture: i64, nPress: i32, x: f64, y: f64, data: i64
    ) {
        let w = gtk_event_controller_get_widget(gesture as GtkEventController)
        if let some(slot) = COMBOS.get(w as i64) {
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
    pub fn linux_combo_released(
        gesture: i64, nPress: i32, x: f64, y: f64, data: i64
    ) {
        let w = gtk_event_controller_get_widget(gesture as GtkEventController)
        if let some(slot) = COMBOS.get(w as i64) {
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
    pub fn linux_combo_scroll(
        ctl: i64, dx: f64, dy: f64, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = COMBOS.get(w as i64) {
            let cb = slot.onMouseWheel
            cb(dx, dy, 0.0, 0.0, false, false, false, false)
        }
        0 as i32
    }
    pub fn linux_combo_key_pressed(
        ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = COMBOS.get(w as i64) {
            let code = keyvalToCode(gdk_keyval_to_lower(keyval))
            let cb = slot.onKeyDown
            cb(code,
               hasMod(state, MOD_CTRL), hasMod(state, MOD_SHIFT),
               hasMod(state, MOD_ALT),  hasMod(state, MOD_META))
        }
        0 as i32
    }
    pub fn linux_combo_key_released(
        ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = COMBOS.get(w as i64) {
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

pub fn createNativeCombo(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let c = gtk_combo_box_text_new()
    gtk_widget_set_size_request(c as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(c as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(c as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(winSlot.fixed, c as GtkWidget, x, y)

    g_signal_connect_data(
        c as GObject,
        cstrFromString("changed"),
        linux_combo_changed,
        0 as *void, 0 as i64, 0 as u32
    )

    let focusCtl = gtk_event_controller_focus_new()
    gtk_widget_add_controller(c as GtkWidget, focusCtl)
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("enter"),
        linux_combo_focus_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("leave"),
        linux_combo_focus_leave, 0 as *void, 0 as i64, 0 as u32
    )

    let motionCtl = gtk_event_controller_motion_new() as GtkEventControllerMotion
    gtk_widget_add_controller(c as GtkWidget, motionCtl as GtkEventController)
    g_signal_connect_data(
        motionCtl as GObject, cstrFromString("enter"),
        linux_combo_motion_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        motionCtl as GObject, cstrFromString("leave"),
        linux_combo_motion_leave, 0 as *void, 0 as i64, 0 as u32
    )

    let click = gtk_gesture_click_new() as GtkGestureClick
    gtk_gesture_single_set_button(click as GtkGestureSingle, 0 as u32)
    gtk_widget_add_controller(c as GtkWidget, click as GtkEventController)
    g_signal_connect_click(
        click as GObject, cstrFromString("pressed"),
        linux_combo_pressed, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_click(
        click as GObject, cstrFromString("released"),
        linux_combo_released, 0 as *void, 0 as i64, 0 as u32
    )

    let scrollCtl = gtk_event_controller_scroll_new(
        GtkEventControllerScrollFlags.vertical | GtkEventControllerScrollFlags.horizontal
    ) as GtkEventControllerScroll
    gtk_widget_add_controller(c as GtkWidget, scrollCtl as GtkEventController)
    g_signal_connect_scroll(
        scrollCtl as GObject, cstrFromString("scroll"),
        linux_combo_scroll, 0 as *void, 0 as i64, 0 as u32
    )

    let keyCtl = gtk_event_controller_key_new() as GtkEventControllerKey
    gtk_widget_add_controller(c as GtkWidget, keyCtl as GtkEventController)
    g_signal_connect_key(
        keyCtl as GObject, cstrFromString("key-pressed"),
        linux_combo_key_pressed, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_key(
        keyCtl as GObject, cstrFromString("key-released"),
        linux_combo_key_released, 0 as *void, 0 as i64, 0 as u32
    )

    let handle = c as i64
    COMBOS[handle] = new ComboSlot(c, winSlot.fixed, windowHandle)
    handle
}

pub fn setOnComboSelect(handle: i64, cb: fn(i32)) {
    if let some(slot) = COMBOS.get(handle) { slot.onSelect = cb }
}
pub fn addComboItem(handle: i64, item: string) {
    if let some(slot) = COMBOS.get(handle) {
        gtk_combo_box_text_append_text(slot.combo, cstrFromString(item))
    }
}
pub fn clearComboItems(handle: i64) {
    if let some(slot) = COMBOS.get(handle) { gtk_combo_box_text_remove_all(slot.combo) }
}
pub fn setComboSelectedIndex(handle: i64, index: i32) {
    if let some(slot) = COMBOS.get(handle) { gtk_combo_box_set_active(slot.combo, index) }
}
pub fn readComboSelectedIndex(handle: i64): i32 {
    match COMBOS.get(handle) {
        some(slot) { gtk_combo_box_get_active(slot.combo) }
        none       { -1 as i32 }
    }
}
pub fn setComboFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = COMBOS.get(handle) {
        gtk_widget_set_size_request(slot.combo as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.combo as GtkWidget, x, y)
    }
}
pub fn setComboEnabled(handle: i64, enabled: bool) {
    if let some(slot) = COMBOS.get(handle) {
        gtk_widget_set_sensitive(slot.combo as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setComboVisible(handle: i64, visible: bool) {
    if let some(slot) = COMBOS.get(handle) {
        gtk_widget_set_visible(slot.combo as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setOnComboFocus(handle: i64, cb: fn()) {
    if let some(slot) = COMBOS.get(handle) { slot.onFocus = cb }
}
pub fn setOnComboBlur(handle: i64, cb: fn()) {
    if let some(slot) = COMBOS.get(handle) { slot.onBlur = cb }
}
pub fn focusCombo(handle: i64) {
    if let some(slot) = COMBOS.get(handle) {
        let _ = gtk_widget_grab_focus(slot.combo as GtkWidget)
    }
}
pub fn blurCombo(handle: i64) {
    if let some(slot) = COMBOS.get(handle) {
        if let some(winSlot) = WINDOWS.get(slot.winHandle) {
            let cur = gtk_window_get_focus(winSlot.win)
            if cur as i64 == slot.combo as i64 {
                gtk_window_set_focus(winSlot.win, 0 as GtkWidget)
            }
        }
    }
}
pub fn setOnComboMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = COMBOS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnComboMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = COMBOS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnComboMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = COMBOS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnComboMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = COMBOS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnComboContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = COMBOS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnComboMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = COMBOS.get(handle) { slot.onMouseWheel = cb }
}
pub fn setOnComboKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = COMBOS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnComboKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = COMBOS.get(handle) { slot.onKeyUp = cb }
}
pub fn setComboTooltip(handle: i64, text: string) {
    if let some(slot) = COMBOS.get(handle) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.combo as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.combo as GtkWidget, cstrFromString(text))
        }
    }
}

} // @extern(C)
