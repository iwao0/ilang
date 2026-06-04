/// Multi-line text input. GtkTextView inside a GtkScrolledWindow.
///
/// **MVP caveats:**
/// - `readTextAreaText` returns the value last written via
///   `setTextAreaText` rather than reading the live buffer back.
///   `gtk_text_buffer_get_text` requires two `GtkTextIter` out-
///   parameters which the GIR generator skipped; wiring those
///   through cleanly is Round 2.
/// - `onChange` likewise dispatches the cached value — the user
///   callback gets the latest committed string but typed edits
///   that haven't flowed back through `setText` won't surface.

use std.ffi { cstrFromString, stringFromCstr }
use gtk {
    GtkWidget, GtkTextView, GtkTextBuffer, GtkScrolledWindow, GtkFixed,
    GtkPolicyType, GObject,
    GtkEventController, GtkEventControllerFocus, GtkEventControllerMotion,
    GtkEventControllerKey, GtkEventControllerScroll, GtkEventControllerScrollFlags,
    GtkGestureSingle, GtkGestureClick,
    gtk_text_view_new, gtk_text_view_get_buffer, gtk_text_view_set_editable,
    gtk_text_buffer_set_text,
    gtk_scrolled_window_new, gtk_scrolled_window_set_child,
    gtk_scrolled_window_set_policy,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible, gtk_widget_set_tooltip_text,
    gtk_widget_grab_focus, gtk_widget_add_controller,
    gtk_event_controller_focus_new, gtk_event_controller_motion_new,
    gtk_event_controller_key_new,
    gtk_event_controller_scroll_new,
    gtk_event_controller_get_widget,
    gtk_event_controller_get_current_event_state,
    gdk_keyval_to_lower,
    gtk_gesture_click_new,
    gtk_gesture_single_set_button, gtk_gesture_single_get_current_button,
    gtk_window_get_focus, gtk_window_set_focus,
    g_signal_connect_data, g_signal_connect_key, g_signal_connect_scroll,
}
use core { WINDOWS, resolveGtkParent, noOpText, noOpClose }
use window { keyvalToCode }

class TextAreaSlot {
    pub view:   GtkTextView
    pub scroll: GtkScrolledWindow
    pub fixed:  GtkFixed
    pub winHandle: i64
    pub cachedText: string
    pub onChange: fn(string)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub onMouseEnter:  fn(f64, f64)
    pub onMouseLeave:  fn(f64, f64)
    pub onMouseDown:   fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:     fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onKeyDown:     fn(string, bool, bool, bool, bool)
    pub onKeyUp:       fn(string, bool, bool, bool, bool)
    pub onMouseWheel:  fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub init(view: GtkTextView, scroll: GtkScrolledWindow, fixed: GtkFixed, winHandle: i64) {
        this.view = view
        this.scroll = scroll
        this.fixed = fixed
        this.winHandle = winHandle
        this.cachedText = ""
        this.onChange = noOpText
        this.onFocus  = noOpClose
        this.onBlur   = noOpClose
        this.onMouseEnter  = taNoOpEnterLeave
        this.onMouseLeave  = taNoOpEnterLeave
        this.onMouseDown   = taNoOpClickMod
        this.onMouseUp     = taNoOpClickMod
        this.onContextMenu = taNoOpClickMod
        this.onKeyDown     = taNoOpKey
        this.onKeyUp       = taNoOpKey
        this.onMouseWheel  = taNoOpWheel
    }
}

fn taNoOpEnterLeave(x: f64, y: f64) {}
fn taNoOpClickMod(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn taNoOpKey(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn taNoOpWheel(dx: f64, dy: f64, x: f64, y: f64, ck: bool, sk: bool, ak: bool, mk: bool) {}

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

let TEXT_AREAS = new Map<i64, TextAreaSlot>()

@extern(C) {
    pub fn linux_textarea_buffer_changed(buf: i64, data: i64) {
        // `data` is the slot's GtkTextView pointer (set when we
        // connected the signal). Look the slot up and forward
        // the cached text — see MVP caveat at top of file.
        if let some(slot) = TEXT_AREAS.get(data) {
            let cb = slot.onChange
            cb(slot.cachedText)
        }
    }
    pub fn linux_textarea_focus_enter(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = TEXT_AREAS.get(w as i64) {
            let cb = slot.onFocus
            cb()
        }
    }
    pub fn linux_textarea_focus_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = TEXT_AREAS.get(w as i64) {
            let cb = slot.onBlur
            cb()
        }
    }
    pub fn linux_textarea_motion_enter(ctl: i64, x: f64, y: f64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = TEXT_AREAS.get(w as i64) {
            let cb = slot.onMouseEnter
            cb(x, y)
        }
    }
    pub fn linux_textarea_motion_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = TEXT_AREAS.get(w as i64) {
            let cb = slot.onMouseLeave
            cb(0.0, 0.0)
        }
    }
    pub fn linux_textarea_pressed(
        gesture: i64, nPress: i32, x: f64, y: f64, data: i64
    ) {
        let w = gtk_event_controller_get_widget(gesture as GtkEventController)
        if let some(slot) = TEXT_AREAS.get(w as i64) {
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
    pub fn linux_textarea_released(
        gesture: i64, nPress: i32, x: f64, y: f64, data: i64
    ) {
        let w = gtk_event_controller_get_widget(gesture as GtkEventController)
        if let some(slot) = TEXT_AREAS.get(w as i64) {
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
    pub fn linux_textarea_scroll(
        ctl: i64, dx: f64, dy: f64, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = TEXT_AREAS.get(w as i64) {
            let cb = slot.onMouseWheel
            cb(dx, dy, 0.0, 0.0, false, false, false, false)
        }
        0 as i32
    }
    pub fn linux_textarea_key_pressed(
        ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = TEXT_AREAS.get(w as i64) {
            let code = keyvalToCode(gdk_keyval_to_lower(keyval))
            let cb = slot.onKeyDown
            cb(code,
               hasMod(state, MOD_CTRL), hasMod(state, MOD_SHIFT),
               hasMod(state, MOD_ALT),  hasMod(state, MOD_META))
        }
        0 as i32
    }
    pub fn linux_textarea_key_released(
        ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = TEXT_AREAS.get(w as i64) {
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

pub fn createNativeTextArea(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let tv = gtk_text_view_new() as GtkTextView
    let buf = gtk_text_view_get_buffer(tv)
    if text != "" {
        gtk_text_buffer_set_text(buf, cstrFromString(text), -1 as i32)
    }
    gtk_text_view_set_editable(tv, if enabled { 1 as i32 } else { 0 as i32 })

    let scroll = gtk_scrolled_window_new() as GtkScrolledWindow
    // GtkPolicyType: automatic = 1
    gtk_scrolled_window_set_policy(scroll, 1 as GtkPolicyType, 1 as GtkPolicyType)
    gtk_scrolled_window_set_child(scroll, tv as GtkWidget)
    gtk_widget_set_size_request(scroll as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(scroll as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(scroll as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, scroll as GtkWidget, x, y)

    let handle = tv as i64
    let slot = new TextAreaSlot(tv, scroll, parent.fixed, windowHandle)
    slot.cachedText = text
    TEXT_AREAS[handle] = slot

    // Buffer "changed" signal — fires after any insert/delete.
    // Pass the textview handle as `data` so the dispatch is O(1).
    g_signal_connect_data(
        buf as GObject, cstrFromString("changed"),
        linux_textarea_buffer_changed, handle as *void, 0 as i64, 0 as u32
    )

    let focusCtl = gtk_event_controller_focus_new()
    gtk_widget_add_controller(tv as GtkWidget, focusCtl)
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("enter"),
        linux_textarea_focus_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("leave"),
        linux_textarea_focus_leave, 0 as *void, 0 as i64, 0 as u32
    )

    let motionCtl = gtk_event_controller_motion_new() as GtkEventControllerMotion
    gtk_widget_add_controller(tv as GtkWidget, motionCtl as GtkEventController)
    g_signal_connect_data(
        motionCtl as GObject, cstrFromString("enter"),
        linux_textarea_motion_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        motionCtl as GObject, cstrFromString("leave"),
        linux_textarea_motion_leave, 0 as *void, 0 as i64, 0 as u32
    )

    let click = gtk_gesture_click_new() as GtkGestureClick
    gtk_gesture_single_set_button(click as GtkGestureSingle, 0 as u32)
    gtk_widget_add_controller(tv as GtkWidget, click as GtkEventController)
    g_signal_connect_data(
        click as GObject, cstrFromString("pressed"),
        linux_textarea_pressed, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        click as GObject, cstrFromString("released"),
        linux_textarea_released, 0 as *void, 0 as i64, 0 as u32
    )

    let scrollCtl = gtk_event_controller_scroll_new(
        GtkEventControllerScrollFlags.vertical | GtkEventControllerScrollFlags.horizontal
    ) as GtkEventControllerScroll
    gtk_widget_add_controller(tv as GtkWidget, scrollCtl as GtkEventController)
    g_signal_connect_scroll(
        scrollCtl as GObject, cstrFromString("scroll"),
        linux_textarea_scroll, 0 as *void, 0 as i64, 0 as u32
    )

    let keyCtl = gtk_event_controller_key_new() as GtkEventControllerKey
    gtk_widget_add_controller(tv as GtkWidget, keyCtl as GtkEventController)
    g_signal_connect_key(
        keyCtl as GObject, cstrFromString("key-pressed"),
        linux_textarea_key_pressed, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_key(
        keyCtl as GObject, cstrFromString("key-released"),
        linux_textarea_key_released, 0 as *void, 0 as i64, 0 as u32
    )

    handle
}

pub fn readTextAreaText(handle: i64): string {
    match TEXT_AREAS.get(handle) {
        some(slot) { slot.cachedText }
        none       { "" }
    }
}
pub fn setTextAreaText(handle: i64, text: string) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        let buf = gtk_text_view_get_buffer(slot.view)
        gtk_text_buffer_set_text(buf, cstrFromString(text), -1 as i32)
        slot.cachedText = text
    }
}
pub fn setTextAreaFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        gtk_widget_set_size_request(slot.scroll as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.scroll as GtkWidget, x, y)
    }
}
pub fn setTextAreaEnabled(handle: i64, enabled: bool) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        gtk_widget_set_sensitive(slot.scroll as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
        gtk_text_view_set_editable(slot.view, if enabled { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setTextAreaVisible(handle: i64, visible: bool) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        gtk_widget_set_visible(slot.scroll as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setTextAreaTooltip(handle: i64, text: string) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.scroll as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.scroll as GtkWidget, cstrFromString(text))
        }
    }
}
pub fn setTextAreaReadOnly(handle: i64, readOnly: bool) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        gtk_text_view_set_editable(slot.view, if readOnly { 0 as i32 } else { 1 as i32 })
    }
}
pub fn focusTextArea(handle: i64) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        let _ = gtk_widget_grab_focus(slot.view as GtkWidget)
    }
}
pub fn blurTextArea(handle: i64) {
    if let some(slot) = TEXT_AREAS.get(handle) {
        if let some(parent) = resolveGtkParent(slot.winHandle) {
            let cur = gtk_window_get_focus(parent.win)
            if cur as i64 == slot.view as i64 {
                gtk_window_set_focus(parent.win, 0 as GtkWidget)
            }
        }
    }
}

pub fn setOnTextAreaChange(handle: i64, cb: fn(string)) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onChange = cb }
}
pub fn setOnTextAreaFocus(handle: i64, cb: fn()) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onFocus = cb }
}
pub fn setOnTextAreaBlur(handle: i64, cb: fn()) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onBlur = cb }
}
pub fn setOnTextAreaMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnTextAreaMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnTextAreaMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnTextAreaMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnTextAreaContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnTextAreaKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnTextAreaKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onKeyUp = cb }
}
pub fn setOnTextAreaMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = TEXT_AREAS.get(handle) { slot.onMouseWheel = cb }
}

} // @extern(C)
