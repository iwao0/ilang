/// Window lifecycle backed by a plain GtkWindow + GMainLoop.
///
/// We deliberately avoid GtkApplication's activate-driven flow:
/// gui-core's user code creates windows eagerly (before
/// runEventLoop), which conflicts with GtkApplication's contract
/// that windows be built inside an `activate` signal handler. The
/// manual path lets the same eager creation pattern work.

use gtk {
use std.ffi { cstrFromString }
    GtkWidget, GtkWindow, GObject, GtkFixed, GMainLoop,
    GtkEventController, GtkEventControllerKey, GtkEventControllerMotion,
    GtkGestureClick, GtkGestureSingle, Keyval,
    gtk_init, gtk_window_new,
    gtk_window_set_title, gtk_window_set_default_size,
    gtk_window_present, gtk_window_set_child, gtk_window_destroy,
    gtk_widget_set_visible, gtk_widget_set_size_request,
    gtk_widget_add_controller,
    gtk_fixed_new,
    gtk_event_controller_key_new, gtk_event_controller_motion_new,
    gtk_gesture_click_new, gtk_gesture_single_set_button,
    gtk_gesture_single_get_current_button,
    gtk_event_controller_get_current_event_state,
    gdk_keyval_to_lower,
    g_main_loop_new, g_main_loop_run, g_main_loop_quit,
    g_signal_connect_data, g_signal_connect_key,
    g_signal_connect_motion, g_signal_connect_click,
}
use core { WINDOWS, WindowSlot, GTK_INITED, LOOP_HOLDER }
use super.events
// GdkModifierType bits we need. Pulled out as locals so each
// dispatch can `& MOD_*` without going through the flags enum.
const MOD_SHIFT: u32 = 0x00000001 as u32
const MOD_CTRL:  u32 = 0x00000004 as u32
const MOD_ALT:   u32 = 0x00000008 as u32
const MOD_META:  u32 = 0x10000000 as u32

fn hasMod(state: u32, mask: u32): bool { (state & mask) != 0 as u32 }

pub fn keyvalToCode(k: u32): string {
    keyvalToEnum(k as Keyval) as string
}

/// GDK keyval (`gtk.Keyval`) → the parent package's `events.KeyCode`
/// (W3C `KeyboardEvent.code`). Unmapped keys fall through to
/// `events.KeyCode.unknown`; the caller then serialises that into
/// the discriminant string for the cross-package callback ABI.
fn keyvalToEnum(k: Keyval): events.KeyCode {
    match k {
        Keyval.keyA       { events.KeyCode.keyA }
        Keyval.keyB       { events.KeyCode.keyB }
        Keyval.keyC       { events.KeyCode.keyC }
        Keyval.keyD       { events.KeyCode.keyD }
        Keyval.keyE       { events.KeyCode.keyE }
        Keyval.keyF       { events.KeyCode.keyF }
        Keyval.keyG       { events.KeyCode.keyG }
        Keyval.keyH       { events.KeyCode.keyH }
        Keyval.keyI       { events.KeyCode.keyI }
        Keyval.keyJ       { events.KeyCode.keyJ }
        Keyval.keyK       { events.KeyCode.keyK }
        Keyval.keyL       { events.KeyCode.keyL }
        Keyval.keyM       { events.KeyCode.keyM }
        Keyval.keyN       { events.KeyCode.keyN }
        Keyval.keyO       { events.KeyCode.keyO }
        Keyval.keyP       { events.KeyCode.keyP }
        Keyval.keyQ       { events.KeyCode.keyQ }
        Keyval.keyR       { events.KeyCode.keyR }
        Keyval.keyS       { events.KeyCode.keyS }
        Keyval.keyT       { events.KeyCode.keyT }
        Keyval.keyU       { events.KeyCode.keyU }
        Keyval.keyV       { events.KeyCode.keyV }
        Keyval.keyW       { events.KeyCode.keyW }
        Keyval.keyX       { events.KeyCode.keyX }
        Keyval.keyY       { events.KeyCode.keyY }
        Keyval.keyZ       { events.KeyCode.keyZ }
        Keyval.digit0     { events.KeyCode.digit0 }
        Keyval.digit1     { events.KeyCode.digit1 }
        Keyval.digit2     { events.KeyCode.digit2 }
        Keyval.digit3     { events.KeyCode.digit3 }
        Keyval.digit4     { events.KeyCode.digit4 }
        Keyval.digit5     { events.KeyCode.digit5 }
        Keyval.digit6     { events.KeyCode.digit6 }
        Keyval.digit7     { events.KeyCode.digit7 }
        Keyval.digit8     { events.KeyCode.digit8 }
        Keyval.digit9     { events.KeyCode.digit9 }
        Keyval.enter      { events.KeyCode.enter }
        Keyval.escape     { events.KeyCode.escape }
        Keyval.backspace  { events.KeyCode.backspace }
        Keyval.tab        { events.KeyCode.tab }
        Keyval.space      { events.KeyCode.space }
        Keyval.arrowLeft  { events.KeyCode.arrowLeft }
        Keyval.arrowRight { events.KeyCode.arrowRight }
        Keyval.arrowDown  { events.KeyCode.arrowDown }
        Keyval.arrowUp    { events.KeyCode.arrowUp }
        _                 { events.KeyCode.unknown }
    }
}

/// `void on_window_destroy(GtkWindow*, gpointer)` — GTK 4 fires
/// `destroy` after the user closes the window. Dispatch the
/// per-window onClose, drop the slot, and quit the main loop when
/// nothing is left.
@extern(C) {
    pub fn linux_window_destroy(win: i64, data: i64) {
        if let some(slot) = WINDOWS.get(win) {
            let cb = slot.onClose
            cb()
            WINDOWS.delete(win)
        }
        if WINDOWS.size() == 0 as i64 {
            if LOOP_HOLDER.length > 0 { g_main_loop_quit(LOOP_HOLDER[0]) }
        }
    }

    /// `gboolean(GtkEventControllerKey*, guint keyval, guint keycode,
    /// GdkModifierType state, gpointer)`. Returns 0 so menubars and
    /// child widgets still get a shot at the keystroke.
    pub fn linux_key_pressed(
        ctrl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        dispatchKey(data, keyval, state, true)
        0 as i32
    }
    pub fn linux_key_released(
        ctrl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        dispatchKey(data, keyval, state, false)
        0 as i32
    }

    /// `void(GtkEventControllerMotion*, gdouble x, gdouble y, gpointer)`.
    pub fn linux_motion(ctrl: i64, x: f64, y: f64, data: i64) {
        if let some(slot) = WINDOWS.get(data) {
            let st = gtk_event_controller_get_current_event_state(
                ctrl as GtkEventController
            )
            let cb = slot.onMouseMove
            cb(x, y, 0 as i32,
               hasMod(st, MOD_CTRL), hasMod(st, MOD_SHIFT),
               hasMod(st, MOD_ALT), hasMod(st, MOD_META))
        }
    }

    /// `void(GtkGestureClick*, gint n_press, gdouble x, gdouble y, gpointer)`.
    pub fn linux_pressed(
        gesture: i64, n_press: i32, x: f64, y: f64, data: i64
    ) {
        dispatchClick(gesture, data, x, y, true)
    }
    pub fn linux_released(
        gesture: i64, n_press: i32, x: f64, y: f64, data: i64
    ) {
        dispatchClick(gesture, data, x, y, false)
    }
}

/// Map gdk button (1=primary, 2=middle, 3=right) to W3C
/// `MouseEvent.button` (0/1/2).
fn buttonToW3C(b: u32): i32 {
    if      b == 1 as u32 { 0 as i32 }
    elif    b == 2 as u32 { 1 as i32 }
    elif    b == 3 as u32 { 2 as i32 }
    else                  { 0 as i32 }
}

@extern(C) {

fn dispatchKey(winH: i64, keyval: u32, state: u32, down: bool) {
    if let some(slot) = WINDOWS.get(winH) {
        let code = keyvalToCode(gdk_keyval_to_lower(keyval))
        let ctrl  = hasMod(state, MOD_CTRL)
        let shift = hasMod(state, MOD_SHIFT)
        let alt   = hasMod(state, MOD_ALT)
        let meta  = hasMod(state, MOD_META)
        let cb = if down { slot.onKeyDown } else { slot.onKeyUp }
        cb(code, ctrl, shift, alt, meta)
    }
}

fn dispatchClick(gesture: i64, winH: i64, x: f64, y: f64, down: bool) {
    if let some(slot) = WINDOWS.get(winH) {
        let btn = buttonToW3C(
            gtk_gesture_single_get_current_button(gesture as GtkGestureSingle)
        )
        let st = gtk_event_controller_get_current_event_state(
            gesture as GtkEventController
        )
        let cb = if down { slot.onMouseDown } else { slot.onMouseUp }
        cb(x, y, btn,
           hasMod(st, MOD_CTRL), hasMod(st, MOD_SHIFT),
           hasMod(st, MOD_ALT), hasMod(st, MOD_META))
    }
}

} // @extern(C)

@extern(C) {

pub fn createNativeWindow(title: string, width: f64, height: f64): i64 {
    if !GTK_INITED[0] {
        gtk_init()
        GTK_INITED[0] = true
    }
    // GIR types `gtk_window_new` as returning the parent `GtkWidget`;
    // every window-side call expects a `GtkWindow`, so down-cast once
    // at construction.
    let win = gtk_window_new() as GtkWindow
    gtk_window_set_title(win, cstrFromString(title))
    gtk_window_set_default_size(win, width as i32, height as i32)

    // Absolute-positioning container — every widget goes through
    // gtk_fixed_put rather than the default child slot.
    let fixed = gtk_fixed_new() as GtkFixed
    gtk_window_set_child(win, fixed as GtkWidget)

    let handle = win as i64
    WINDOWS[handle] = new WindowSlot(win, fixed)
    g_signal_connect_data(
        win as GObject,
        cstrFromString("destroy"),
        linux_window_destroy,
        0 as *void,
        0 as i64,
        0 as u32
    )

    // Key controller — emits "key-pressed" / "key-released" on
    // every keystroke while the window has focus.
    let keyCtrl = gtk_event_controller_key_new() as GtkEventControllerKey
    g_signal_connect_key(
        keyCtrl as GObject, cstrFromString("key-pressed"),
        linux_key_pressed, handle as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_key(
        keyCtrl as GObject, cstrFromString("key-released"),
        linux_key_released, handle as *void, 0 as i64, 0 as u32
    )
    gtk_widget_add_controller(
        win as GtkWidget, keyCtrl as GtkEventController
    )

    // Pointer motion controller.
    let motionCtrl = gtk_event_controller_motion_new() as GtkEventControllerMotion
    g_signal_connect_motion(
        motionCtrl as GObject, cstrFromString("motion"),
        linux_motion, handle as *void, 0 as i64, 0 as u32
    )
    gtk_widget_add_controller(
        win as GtkWidget, motionCtrl as GtkEventController
    )

    // Click gesture — `button=0` matches every button so the
    // handler can report which one fired via
    // `gtk_gesture_single_get_current_button`.
    let click = gtk_gesture_click_new() as GtkGestureClick
    gtk_gesture_single_set_button(click as GtkGestureSingle, 0 as u32)
    g_signal_connect_click(
        click as GObject, cstrFromString("pressed"),
        linux_pressed, handle as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_click(
        click as GObject, cstrFromString("released"),
        linux_released, handle as *void, 0 as i64, 0 as u32
    )
    gtk_widget_add_controller(
        win as GtkWidget, click as GtkEventController
    )

    handle
}

pub fn setWindowTitle(handle: i64, title: string) {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_set_title(slot.win, cstrFromString(title))
    }
}

pub fn setWindowSize(handle: i64, width: f64, height: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_set_default_size(slot.win, width as i32, height as i32)
    }
}

pub fn showNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_present(slot.win)
    }
}

pub fn hideNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_widget_set_visible(slot.win as GtkWidget, 0 as i32)
    }
}

pub fn runEventLoop() {
    if !GTK_INITED[0] { return }
    if LOOP_HOLDER.length == 0 {
        LOOP_HOLDER.push(g_main_loop_new(0 as *void, 0 as i32))
    }
    g_main_loop_run(LOOP_HOLDER[0])
}

} // @extern(C)
