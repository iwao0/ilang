/// GtkLabel wrapper.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkLabel, GtkFixed, GObject,
    GtkEventController, GtkEventControllerMotion,
    GtkEventControllerScroll, GtkEventControllerScrollFlags,
    GtkGestureSingle, GtkGestureClick,
    gtk_label_new, gtk_label_set_text,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_visible,
    gtk_widget_set_tooltip_text,
    gtk_widget_add_controller,
    gtk_event_controller_motion_new,
    gtk_event_controller_scroll_new,
    gtk_event_controller_get_widget,
    gtk_event_controller_get_current_event_state,
    gtk_gesture_click_new,
    gtk_gesture_single_set_button, gtk_gesture_single_get_current_button,
    g_signal_connect_data, g_signal_connect_scroll,
}
use core { WINDOWS }

class LabelSlot {
    pub label: GtkLabel
    pub fixed: GtkFixed
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub init(label: GtkLabel, fixed: GtkFixed) {
        this.label = label
        this.fixed = fixed
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseDown  = noOpClickMod
        this.onMouseUp    = noOpClickMod
        this.onContextMenu = noOpClickMod
        this.onMouseWheel = noOpWheel
    }
}

fn noOpEnterLeave(x: f64, y: f64) {}
fn noOpClickMod(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn noOpWheel(dx: f64, dy: f64, x: f64, y: f64, ck: bool, sk: bool, ak: bool, mk: bool) {}

// GdkModifierType bits — pulled out so the gesture handlers can
// `& MOD_*` without going through the flags enum.
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

let LABELS = new Map<i64, LabelSlot>()

@extern(C) {

pub fn linux_label_motion_enter(ctl: i64, x: f64, y: f64, data: i64) {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = LABELS.get(w as i64) {
        let cb = slot.onMouseEnter
        cb(x, y)
    }
}
pub fn linux_label_motion_leave(ctl: i64, data: i64) {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = LABELS.get(w as i64) {
        let cb = slot.onMouseLeave
        cb(0.0, 0.0)
    }
}
pub fn linux_label_pressed(
    gesture: i64, nPress: i32, x: f64, y: f64, data: i64
) {
    let w = gtk_event_controller_get_widget(gesture as GtkEventController)
    if let some(slot) = LABELS.get(w as i64) {
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
pub fn linux_label_released(
    gesture: i64, nPress: i32, x: f64, y: f64, data: i64
) {
    let w = gtk_event_controller_get_widget(gesture as GtkEventController)
    if let some(slot) = LABELS.get(w as i64) {
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
pub fn linux_label_scroll(
    ctrl: i64, dx: f64, dy: f64, data: i64
): i32 {
    let w = gtk_event_controller_get_widget(ctrl as GtkEventController)
    if let some(slot) = LABELS.get(w as i64) {
        let cb = slot.onMouseWheel
        cb(dx, dy, 0.0, 0.0, false, false, false, false)
    }
    0 as i32
}

}

@extern(C) {

pub fn createNativeLabel(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    visible: bool, selectable: bool
): i64 {
    // `selectable` is honoured on macOS (NSTextField). GtkLabel
    // can expose `set_selectable` too, but we don't bind it yet
    // and the cursor stays as the default arrow regardless.
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let label = gtk_label_new(cstrFromString(text)) as GtkLabel
    gtk_widget_set_size_request(label as GtkWidget, width as i32, height as i32)
    gtk_widget_set_visible(
        label as GtkWidget,
        if visible { 1 as i32 } else { 0 as i32 }
    )
    gtk_fixed_put(winSlot.fixed, label as GtkWidget, x, y)

    // Motion controller for onMouseEnter / onMouseLeave.
    let motionCtl = gtk_event_controller_motion_new() as GtkEventControllerMotion
    gtk_widget_add_controller(label as GtkWidget, motionCtl as GtkEventController)
    g_signal_connect_data(
        motionCtl as GObject, cstrFromString("enter"),
        linux_label_motion_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        motionCtl as GObject, cstrFromString("leave"),
        linux_label_motion_leave, 0 as *void, 0 as i64, 0 as u32
    )

    // Click gesture for onMouseDown / onMouseUp + onContextMenu
    // (the latter fires inline on a right-button press). Set
    // button=0 to listen for every mouse button — the handler
    // reads the actual one back via the gesture.
    let clickGesture = gtk_gesture_click_new() as GtkGestureClick
    gtk_gesture_single_set_button(clickGesture as GtkGestureSingle, 0 as u32)
    gtk_widget_add_controller(label as GtkWidget, clickGesture as GtkEventController)
    g_signal_connect_data(
        clickGesture as GObject, cstrFromString("pressed"),
        linux_label_pressed, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        clickGesture as GObject, cstrFromString("released"),
        linux_label_released, 0 as *void, 0 as i64, 0 as u32
    )

    // Scroll controller for onMouseWheel.
    let scrollCtl = gtk_event_controller_scroll_new(
        GtkEventControllerScrollFlags.vertical | GtkEventControllerScrollFlags.horizontal
    ) as GtkEventControllerScroll
    gtk_widget_add_controller(label as GtkWidget, scrollCtl as GtkEventController)
    g_signal_connect_scroll(
        scrollCtl as GObject, cstrFromString("scroll"),
        linux_label_scroll, 0 as *void, 0 as i64, 0 as u32
    )

    let handle = label as i64
    LABELS[handle] = new LabelSlot(label, winSlot.fixed)
    handle
}

pub fn setLabelText(handle: i64, text: string) {
    if let some(slot) = LABELS.get(handle) {
        gtk_label_set_text(slot.label, cstrFromString(text))
    }
}

pub fn setLabelFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = LABELS.get(handle) {
        gtk_widget_set_size_request(slot.label as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.label as GtkWidget, x, y)
    }
}

pub fn setLabelVisible(handle: i64, visible: bool) {
    if let some(slot) = LABELS.get(handle) {
        gtk_widget_set_visible(
            slot.label as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}
pub fn setLabelSelectable(handle: i64, selectable: bool) {
    // No-op for now — gtk_label_set_selectable isn't bound yet.
}
pub fn setLabelTooltip(handle: i64, text: string) {
    if let some(slot) = LABELS.get(handle) {
        // Empty string → pass NULL so GTK clears the tooltip
        // outright (`gtk_widget_set_tooltip_text` documents NULL
        // as the "remove tooltip" path; an empty cstring would
        // still register an empty tooltip).
        if text == "" {
            gtk_widget_set_tooltip_text(slot.label as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.label as GtkWidget, cstrFromString(text))
        }
    }
}

pub fn setOnLabelMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = LABELS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnLabelMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = LABELS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnLabelMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = LABELS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnLabelMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = LABELS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnLabelContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = LABELS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnLabelMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = LABELS.get(handle) { slot.onMouseWheel = cb }
}

} // @extern(C)
