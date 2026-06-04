/// Calendar date picker — `GtkCalendar`. Month indices in GTK4
/// are 0..11; we translate to/from the public 1..12 convention.
///
/// `setDatePickerMin` / `setDatePickerMax` are no-ops on GTK4
/// — `GtkCalendar` doesn't surface a clamp range. The public
/// `DatePicker.min` / `.max` still update the ilang-side state
/// so other backends honour them.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkCalendar, GtkFixed, GObject,
    GtkEventControllerFocus, GtkEventControllerMotion,
    GtkGestureClick, GtkGestureSingle, GtkEventControllerScroll,
    GtkEventControllerScrollFlags, GtkEventControllerKey,
    GtkEventController,
    gtk_calendar_new,
    gtk_calendar_get_year, gtk_calendar_get_month, gtk_calendar_get_day,
    gtk_calendar_set_year, gtk_calendar_set_month, gtk_calendar_set_day,
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
use core { resolveGtkParent }

class DateSlot {
    pub cal:       GtkCalendar
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
    pub init(cal: GtkCalendar, fixed: GtkFixed, winHandle: i64) {
        this.cal       = cal
        this.fixed     = fixed
        this.winHandle = winHandle
        this.onChange = dpNoOpYmd
        this.onFocus  = dpNoOp
        this.onBlur   = dpNoOp
        this.onMouseEnter = dpNoOpEnter
        this.onMouseLeave = dpNoOpEnter
        this.onMouseDown  = dpNoOpClick
        this.onMouseUp    = dpNoOpClick
        this.onContextMenu = dpNoOpClick
        this.onMouseWheel = dpNoOpWheel
        this.onKeyDown = dpNoOpKey
        this.onKeyUp   = dpNoOpKey
    }
}

fn dpNoOpYmd(y: i32, m: i32, d: i32) {}
fn dpNoOp() {}
fn dpNoOpEnter(x: f64, y: f64) {}
fn dpNoOpClick(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn dpNoOpWheel(dx: f64, dy: f64, x: f64, y: f64, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn dpNoOpKey(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {}

const DP_MOD_SHIFT: u32 = 0x00000001 as u32
const DP_MOD_CTRL:  u32 = 0x00000004 as u32
const DP_MOD_ALT:   u32 = 0x00000008 as u32
const DP_MOD_META:  u32 = 0x10000000 as u32
fn dpHasMod(state: u32, mask: u32): bool { (state & mask) != 0 as u32 }
fn dpGdkBtnToW3c(gdk: u32): i32 {
    match gdk {
        1 as u32 { 0 as i32 }
        2 as u32 { 1 as i32 }
        3 as u32 { 2 as i32 }
        _        { 0 as i32 }
    }
}

let DATE_PICKERS = new Map<i64, DateSlot>()

@extern(C) {

pub fn linux_dp_day_selected(cal: i64, data: i64) {
    if let some(slot) = DATE_PICKERS.get(cal) {
        let y = gtk_calendar_get_year(slot.cal)
        // GTK4 month is 0..11; translate to public 1..12.
        let m = gtk_calendar_get_month(slot.cal) + (1 as i32)
        let d = gtk_calendar_get_day(slot.cal)
        let cb = slot.onChange
        cb(y, m, d)
    }
}

pub fn linux_dp_focus_enter(ctl: i64, data: i64) {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = DATE_PICKERS.get(w as i64) {
        let cb = slot.onFocus; cb()
    }
}
pub fn linux_dp_focus_leave(ctl: i64, data: i64) {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = DATE_PICKERS.get(w as i64) {
        let cb = slot.onBlur; cb()
    }
}
pub fn linux_dp_motion_enter(ctl: i64, x: f64, y: f64, data: i64) {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = DATE_PICKERS.get(w as i64) {
        let cb = slot.onMouseEnter; cb(x, y)
    }
}
pub fn linux_dp_motion_leave(ctl: i64, data: i64) {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = DATE_PICKERS.get(w as i64) {
        let cb = slot.onMouseLeave; cb(0.0, 0.0)
    }
}
pub fn linux_dp_pressed(gesture: i64, nPress: i32, x: f64, y: f64, data: i64) {
    let w = gtk_event_controller_get_widget(gesture as GtkEventController)
    if let some(slot) = DATE_PICKERS.get(w as i64) {
        let btn = gtk_gesture_single_get_current_button(gesture as GtkGestureSingle)
        let w3c = dpGdkBtnToW3c(btn)
        let st = gtk_event_controller_get_current_event_state(
            gesture as GtkEventController
        ) as u32
        let cb = slot.onMouseDown
        cb(x, y, w3c,
           dpHasMod(st, DP_MOD_CTRL), dpHasMod(st, DP_MOD_SHIFT),
           dpHasMod(st, DP_MOD_ALT),  dpHasMod(st, DP_MOD_META))
        if btn == 3 as u32 {
            let ctx = slot.onContextMenu
            ctx(x, y, w3c,
                dpHasMod(st, DP_MOD_CTRL), dpHasMod(st, DP_MOD_SHIFT),
                dpHasMod(st, DP_MOD_ALT),  dpHasMod(st, DP_MOD_META))
        }
    }
}
pub fn linux_dp_released(gesture: i64, nPress: i32, x: f64, y: f64, data: i64) {
    let w = gtk_event_controller_get_widget(gesture as GtkEventController)
    if let some(slot) = DATE_PICKERS.get(w as i64) {
        let btn = gtk_gesture_single_get_current_button(gesture as GtkGestureSingle)
        let st = gtk_event_controller_get_current_event_state(
            gesture as GtkEventController
        ) as u32
        let cb = slot.onMouseUp
        cb(x, y, dpGdkBtnToW3c(btn),
           dpHasMod(st, DP_MOD_CTRL), dpHasMod(st, DP_MOD_SHIFT),
           dpHasMod(st, DP_MOD_ALT),  dpHasMod(st, DP_MOD_META))
    }
}
pub fn linux_dp_scroll(ctl: i64, dx: f64, dy: f64, data: i64): i32 {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = DATE_PICKERS.get(w as i64) {
        let cb = slot.onMouseWheel
        cb(dx, dy, 0.0, 0.0, false, false, false, false)
    }
    0 as i32
}
pub fn linux_dp_key_pressed(
    ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
): i32 {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = DATE_PICKERS.get(w as i64) {
        let code = keyvalToCode(gdk_keyval_to_lower(keyval))
        let cb = slot.onKeyDown
        cb(code,
           dpHasMod(state, DP_MOD_CTRL), dpHasMod(state, DP_MOD_SHIFT),
           dpHasMod(state, DP_MOD_ALT),  dpHasMod(state, DP_MOD_META))
    }
    0 as i32
}
pub fn linux_dp_key_released(
    ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
): i32 {
    let w = gtk_event_controller_get_widget(ctl as GtkEventController)
    if let some(slot) = DATE_PICKERS.get(w as i64) {
        let code = keyvalToCode(gdk_keyval_to_lower(keyval))
        let cb = slot.onKeyUp
        cb(code,
           dpHasMod(state, DP_MOD_CTRL), dpHasMod(state, DP_MOD_SHIFT),
           dpHasMod(state, DP_MOD_ALT),  dpHasMod(state, DP_MOD_META))
    }
    0 as i32
}

pub fn createNativeDatePicker(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let cal = gtk_calendar_new() as GtkCalendar
    gtk_widget_set_size_request(cal as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(cal as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(cal as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, cal as GtkWidget, x, y)

    g_signal_connect_data(
        cal as GObject, cstrFromString("day-selected"),
        linux_dp_day_selected, 0 as *void, 0 as i64, 0 as u32
    )

    let focusCtl = gtk_event_controller_focus_new()
    gtk_widget_add_controller(cal as GtkWidget, focusCtl)
    g_signal_connect_data(focusCtl as GObject, cstrFromString("enter"),
        linux_dp_focus_enter, 0 as *void, 0 as i64, 0 as u32)
    g_signal_connect_data(focusCtl as GObject, cstrFromString("leave"),
        linux_dp_focus_leave, 0 as *void, 0 as i64, 0 as u32)

    let motionCtl = gtk_event_controller_motion_new() as GtkEventControllerMotion
    gtk_widget_add_controller(cal as GtkWidget, motionCtl as GtkEventController)
    g_signal_connect_data(motionCtl as GObject, cstrFromString("enter"),
        linux_dp_motion_enter, 0 as *void, 0 as i64, 0 as u32)
    g_signal_connect_data(motionCtl as GObject, cstrFromString("leave"),
        linux_dp_motion_leave, 0 as *void, 0 as i64, 0 as u32)

    let click = gtk_gesture_click_new() as GtkGestureClick
    gtk_gesture_single_set_button(click as GtkGestureSingle, 0 as u32)
    gtk_widget_add_controller(cal as GtkWidget, click as GtkEventController)
    g_signal_connect_click(click as GObject, cstrFromString("pressed"),
        linux_dp_pressed, 0 as *void, 0 as i64, 0 as u32)
    g_signal_connect_click(click as GObject, cstrFromString("released"),
        linux_dp_released, 0 as *void, 0 as i64, 0 as u32)

    let scrollCtl = gtk_event_controller_scroll_new(
        GtkEventControllerScrollFlags.vertical | GtkEventControllerScrollFlags.horizontal
    ) as GtkEventControllerScroll
    gtk_widget_add_controller(cal as GtkWidget, scrollCtl as GtkEventController)
    g_signal_connect_scroll(scrollCtl as GObject, cstrFromString("scroll"),
        linux_dp_scroll, 0 as *void, 0 as i64, 0 as u32)

    let keyCtl = gtk_event_controller_key_new() as GtkEventControllerKey
    gtk_widget_add_controller(cal as GtkWidget, keyCtl as GtkEventController)
    g_signal_connect_key(keyCtl as GObject, cstrFromString("key-pressed"),
        linux_dp_key_pressed, 0 as *void, 0 as i64, 0 as u32)
    g_signal_connect_key(keyCtl as GObject, cstrFromString("key-released"),
        linux_dp_key_released, 0 as *void, 0 as i64, 0 as u32)

    let handle = cal as i64
    DATE_PICKERS.set(handle, new DateSlot(cal, parent.fixed, windowHandle))
    handle
}

pub fn setDatePickerValue(host: i64, year: i32, month: i32, day: i32) {
    if let some(slot) = DATE_PICKERS.get(host) {
        gtk_calendar_set_year(slot.cal, year)
        // Public month 1..12 → GTK 0..11.
        gtk_calendar_set_month(slot.cal, month - (1 as i32))
        gtk_calendar_set_day(slot.cal, day)
    }
}

pub fn readDatePickerYear(host: i64): i32 {
    match DATE_PICKERS.get(host) {
        some(slot) { gtk_calendar_get_year(slot.cal) }
        none       { 1970 as i32 }
    }
}
pub fn readDatePickerMonth(host: i64): i32 {
    match DATE_PICKERS.get(host) {
        some(slot) { gtk_calendar_get_month(slot.cal) + (1 as i32) }
        none       { 1 as i32 }
    }
}
pub fn readDatePickerDay(host: i64): i32 {
    match DATE_PICKERS.get(host) {
        some(slot) { gtk_calendar_get_day(slot.cal) }
        none       { 1 as i32 }
    }
}

pub fn setDatePickerMin(host: i64, year: i32, month: i32, day: i32) {
    // GtkCalendar has no built-in min/max clamp; documented gap.
    let _ = host; let _ = year; let _ = month; let _ = day
}
pub fn setDatePickerMax(host: i64, year: i32, month: i32, day: i32) {
    let _ = host; let _ = year; let _ = month; let _ = day
}

pub fn setDatePickerFrame(host: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = DATE_PICKERS.get(host) {
        gtk_widget_set_size_request(slot.cal as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.cal as GtkWidget, x, y)
    }
}
pub fn setDatePickerEnabled(host: i64, enabled: bool) {
    if let some(slot) = DATE_PICKERS.get(host) {
        gtk_widget_set_sensitive(slot.cal as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setDatePickerVisible(host: i64, visible: bool) {
    if let some(slot) = DATE_PICKERS.get(host) {
        gtk_widget_set_visible(slot.cal as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setDatePickerTooltip(host: i64, text: string) {
    if let some(slot) = DATE_PICKERS.get(host) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.cal as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.cal as GtkWidget, cstrFromString(text))
        }
    }
}

pub fn setOnDatePickerChange(host: i64, cb: fn(i32, i32, i32)) {
    if let some(slot) = DATE_PICKERS.get(host) { slot.onChange = cb }
}
pub fn setOnDatePickerFocus(host: i64, cb: fn()) {
    if let some(slot) = DATE_PICKERS.get(host) { slot.onFocus = cb }
}
pub fn setOnDatePickerBlur(host: i64, cb: fn()) {
    if let some(slot) = DATE_PICKERS.get(host) { slot.onBlur = cb }
}
pub fn setOnDatePickerMouseEnter(host: i64, cb: fn(f64, f64)) {
    if let some(slot) = DATE_PICKERS.get(host) { slot.onMouseEnter = cb }
}
pub fn setOnDatePickerMouseLeave(host: i64, cb: fn(f64, f64)) {
    if let some(slot) = DATE_PICKERS.get(host) { slot.onMouseLeave = cb }
}
pub fn setOnDatePickerMouseDown(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { if let some(slot) = DATE_PICKERS.get(host) { slot.onMouseDown = cb } }
pub fn setOnDatePickerMouseUp(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { if let some(slot) = DATE_PICKERS.get(host) { slot.onMouseUp = cb } }
pub fn setOnDatePickerContextMenu(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { if let some(slot) = DATE_PICKERS.get(host) { slot.onContextMenu = cb } }
pub fn setOnDatePickerMouseWheel(
    host: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) { if let some(slot) = DATE_PICKERS.get(host) { slot.onMouseWheel = cb } }
pub fn setOnDatePickerKeyDown(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = DATE_PICKERS.get(host) { slot.onKeyDown = cb }
}
pub fn setOnDatePickerKeyUp(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = DATE_PICKERS.get(host) { slot.onKeyUp = cb }
}

pub fn focusDatePicker(host: i64) {
    if let some(slot) = DATE_PICKERS.get(host) {
        let _ = gtk_widget_grab_focus(slot.cal as GtkWidget)
    }
}
pub fn blurDatePicker(host: i64) {
    let _ = host
}

} // @extern(C)
