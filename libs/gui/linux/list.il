/// GtkListBox + GtkScrolledWindow wrapper. Each ilang `addListItem`
/// creates a GtkLabel and appends it as a row.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkLabel, GtkListBox, GtkListBoxRow, GtkScrolledWindow,
    GtkFixed, GObject,
    GtkEventController, GtkEventControllerFocus, GtkEventControllerMotion,
    GtkEventControllerScroll, GtkEventControllerScrollFlags,
    GtkEventControllerKey,
    GtkGestureSingle, GtkGestureClick,
    gtk_label_new,
    gtk_list_box_new, gtk_list_box_append, gtk_list_box_remove,
    gtk_list_box_select_row, gtk_list_box_unselect_all,
    gtk_list_box_get_selected_row, gtk_list_box_get_row_at_index,
    gtk_list_box_row_get_index,
    gtk_list_box_row_new, gtk_list_box_row_set_child,
    gtk_scrolled_window_new, gtk_scrolled_window_set_child,
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
    g_signal_connect_data, g_signal_connect_data2, g_signal_connect_key,
    g_signal_connect_motion, g_signal_connect_click, g_signal_connect_scroll,
}
use window { keyvalToCode }
use core { WINDOWS, noOp1, noOpI32 }

class ListSlot {
    pub scroll: GtkScrolledWindow
    pub listBox: GtkListBox
    pub fixed:   GtkFixed
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
    pub init(scroll: GtkScrolledWindow, lb: GtkListBox, fixed: GtkFixed,
             winHandle: i64) {
        this.scroll = scroll
        this.listBox = lb
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

let LISTS = new Map<i64, ListSlot>()

@extern(C) {
    pub fn linux_list_row_selected(lb: i64, row: i64, data: i64) {
        let key = lb as i64
        if let some(slot) = LISTS.get(key) {
            let r = gtk_list_box_get_selected_row(slot.listBox)
            let idx: i32 = if r as i64 == 0 as i64 {
                -1 as i32
            } else {
                gtk_list_box_row_get_index(r)
            }
            let cb = slot.onSelect
            cb(idx)
        }
    }
    pub fn linux_list_focus_enter(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = LISTS.get(w as i64) {
            let cb = slot.onFocus
            cb()
        }
    }
    pub fn linux_list_focus_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = LISTS.get(w as i64) {
            let cb = slot.onBlur
            cb()
        }
    }
    pub fn linux_list_motion_enter(ctl: i64, x: f64, y: f64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = LISTS.get(w as i64) {
            let cb = slot.onMouseEnter
            cb(x, y)
        }
    }
    pub fn linux_list_motion_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = LISTS.get(w as i64) {
            let cb = slot.onMouseLeave
            cb(0.0, 0.0)
        }
    }
    pub fn linux_list_pressed(
        gesture: i64, nPress: i32, x: f64, y: f64, data: i64
    ) {
        let w = gtk_event_controller_get_widget(gesture as GtkEventController)
        if let some(slot) = LISTS.get(w as i64) {
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
    pub fn linux_list_released(
        gesture: i64, nPress: i32, x: f64, y: f64, data: i64
    ) {
        let w = gtk_event_controller_get_widget(gesture as GtkEventController)
        if let some(slot) = LISTS.get(w as i64) {
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
    pub fn linux_list_scroll(
        ctl: i64, dx: f64, dy: f64, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = LISTS.get(w as i64) {
            let cb = slot.onMouseWheel
            cb(dx, dy, 0.0, 0.0, false, false, false, false)
        }
        0 as i32
    }
    pub fn linux_list_key_pressed(
        ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = LISTS.get(w as i64) {
            let code = keyvalToCode(gdk_keyval_to_lower(keyval))
            let cb = slot.onKeyDown
            cb(code,
               hasMod(state, MOD_CTRL), hasMod(state, MOD_SHIFT),
               hasMod(state, MOD_ALT),  hasMod(state, MOD_META))
        }
        0 as i32
    }
    pub fn linux_list_key_released(
        ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = LISTS.get(w as i64) {
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

pub fn createNativeList(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let scroll = gtk_scrolled_window_new() as GtkScrolledWindow
    let lb     = gtk_list_box_new() as GtkListBox
    gtk_scrolled_window_set_child(scroll, lb as GtkWidget)
    gtk_widget_set_size_request(scroll as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(lb as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(scroll as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(winSlot.fixed, scroll as GtkWidget, x, y)

    g_signal_connect_data2(
        lb as GObject,
        cstrFromString("row-selected"),
        linux_list_row_selected,
        0 as *void, 0 as i64, 0 as u32
    )

    // Focus controller attached to the GtkListBox — that's what
    // takes keyboard focus when the user Tabs into the list.
    let focusCtl = gtk_event_controller_focus_new()
    gtk_widget_add_controller(lb as GtkWidget, focusCtl)
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("enter"),
        linux_list_focus_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("leave"),
        linux_list_focus_leave, 0 as *void, 0 as i64, 0 as u32
    )

    let motionCtl = gtk_event_controller_motion_new() as GtkEventControllerMotion
    gtk_widget_add_controller(lb as GtkWidget, motionCtl as GtkEventController)
    g_signal_connect_data(
        motionCtl as GObject, cstrFromString("enter"),
        linux_list_motion_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        motionCtl as GObject, cstrFromString("leave"),
        linux_list_motion_leave, 0 as *void, 0 as i64, 0 as u32
    )

    let click = gtk_gesture_click_new() as GtkGestureClick
    gtk_gesture_single_set_button(click as GtkGestureSingle, 0 as u32)
    gtk_widget_add_controller(lb as GtkWidget, click as GtkEventController)
    g_signal_connect_click(
        click as GObject, cstrFromString("pressed"),
        linux_list_pressed, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_click(
        click as GObject, cstrFromString("released"),
        linux_list_released, 0 as *void, 0 as i64, 0 as u32
    )

    let scrollCtl = gtk_event_controller_scroll_new(
        GtkEventControllerScrollFlags.vertical | GtkEventControllerScrollFlags.horizontal
    ) as GtkEventControllerScroll
    gtk_widget_add_controller(lb as GtkWidget, scrollCtl as GtkEventController)
    g_signal_connect_scroll(
        scrollCtl as GObject, cstrFromString("scroll"),
        linux_list_scroll, 0 as *void, 0 as i64, 0 as u32
    )

    let keyCtl = gtk_event_controller_key_new() as GtkEventControllerKey
    gtk_widget_add_controller(lb as GtkWidget, keyCtl as GtkEventController)
    g_signal_connect_key(
        keyCtl as GObject, cstrFromString("key-pressed"),
        linux_list_key_pressed, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_key(
        keyCtl as GObject, cstrFromString("key-released"),
        linux_list_key_released, 0 as *void, 0 as i64, 0 as u32
    )

    // The GtkListBox is what fires `row-selected`, so the public
    // handle keys off its pointer rather than the ScrolledWindow.
    let handle = lb as i64
    LISTS[handle] = new ListSlot(scroll, lb, winSlot.fixed, windowHandle)
    handle
}

pub fn setOnListSelect(handle: i64, cb: fn(i32)) {
    if let some(slot) = LISTS.get(handle) { slot.onSelect = cb }
}

pub fn addListItem(handle: i64, item: string) {
    if let some(slot) = LISTS.get(handle) {
        let label = gtk_label_new(cstrFromString(item)) as GtkLabel
        let row = gtk_list_box_row_new() as GtkListBoxRow
        gtk_list_box_row_set_child(row, label as GtkWidget)
        gtk_list_box_append(slot.listBox, row as GtkWidget)
    }
}

pub fn clearListItems(handle: i64) {
    if let some(slot) = LISTS.get(handle) {
        // Walk from index 0 — each remove shifts the remaining rows
        // down, so the next "row at 0" is the next surviving row.
        let row = gtk_list_box_get_row_at_index(slot.listBox, 0 as i32)
        while row as i64 != 0 as i64 {
            gtk_list_box_remove(slot.listBox, row as GtkWidget)
            row = gtk_list_box_get_row_at_index(slot.listBox, 0 as i32)
        }
    }
}

pub fn setListSelectedIndex(handle: i64, index: i32) {
    if let some(slot) = LISTS.get(handle) {
        if index < 0 as i32 {
            gtk_list_box_unselect_all(slot.listBox)
        } else {
            let row = gtk_list_box_get_row_at_index(slot.listBox, index)
            if row as i64 != 0 as i64 {
                gtk_list_box_select_row(slot.listBox, row)
            }
        }
    }
}

pub fn readListSelectedIndex(handle: i64): i32 {
    match LISTS.get(handle) {
        some(slot) {
            let r = gtk_list_box_get_selected_row(slot.listBox)
            if r as i64 == 0 as i64 { -1 as i32 } else { gtk_list_box_row_get_index(r) }
        }
        none { -1 as i32 }
    }
}

pub fn setListFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = LISTS.get(handle) {
        gtk_widget_set_size_request(slot.scroll as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.scroll as GtkWidget, x, y)
    }
}

pub fn setListEnabled(handle: i64, enabled: bool) {
    if let some(slot) = LISTS.get(handle) {
        gtk_widget_set_sensitive(
            slot.listBox as GtkWidget,
            if enabled { 1 as i32 } else { 0 as i32 }
        )
    }
}

pub fn setListVisible(handle: i64, visible: bool) {
    if let some(slot) = LISTS.get(handle) {
        gtk_widget_set_visible(
            slot.scroll as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}
pub fn setOnListFocus(handle: i64, cb: fn()) {
    if let some(slot) = LISTS.get(handle) { slot.onFocus = cb }
}
pub fn setOnListBlur(handle: i64, cb: fn()) {
    if let some(slot) = LISTS.get(handle) { slot.onBlur = cb }
}
pub fn focusList(handle: i64) {
    if let some(slot) = LISTS.get(handle) {
        let _ = gtk_widget_grab_focus(slot.listBox as GtkWidget)
    }
}
pub fn blurList(handle: i64) {
    if let some(slot) = LISTS.get(handle) {
        if let some(winSlot) = WINDOWS.get(slot.winHandle) {
            let cur = gtk_window_get_focus(winSlot.win)
            if cur as i64 == slot.listBox as i64 {
                gtk_window_set_focus(winSlot.win, 0 as GtkWidget)
            }
        }
    }
}
pub fn setOnListMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = LISTS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnListMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = LISTS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnListMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = LISTS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnListMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = LISTS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnListContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = LISTS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnListMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = LISTS.get(handle) { slot.onMouseWheel = cb }
}
pub fn setOnListKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = LISTS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnListKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = LISTS.get(handle) { slot.onKeyUp = cb }
}
pub fn setListTooltip(handle: i64, text: string) {
    if let some(slot) = LISTS.get(handle) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.scroll as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.scroll as GtkWidget, cstrFromString(text))
        }
    }
}

} // @extern(C)
