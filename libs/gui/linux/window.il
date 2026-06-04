/// Window lifecycle backed by a plain GtkWindow + GMainLoop.
///
/// We deliberately avoid GtkApplication's activate-driven flow:
/// gui-core's user code creates windows eagerly (before
/// runEventLoop), which conflicts with GtkApplication's contract
/// that windows be built inside an `activate` signal handler. The
/// manual path lets the same eager creation pattern work.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkWindow, GObject, GtkFixed, GMainLoop,
    GtkEventController, GtkEventControllerKey, GtkEventControllerMotion,
    GtkGestureClick, GtkGestureSingle, Keyval,
    gtk_init, gtk_window_new,
    gtk_window_set_title, gtk_window_set_default_size,
    gtk_window_present, gtk_window_set_child, gtk_window_destroy,
    gtk_window_minimize, gtk_window_unminimize,
    gtk_window_maximize, gtk_window_unmaximize,
    gtk_window_fullscreen, gtk_window_unfullscreen,
    gtk_window_is_maximized, gtk_window_is_fullscreen,
    gtk_window_set_resizable, gtk_window_get_resizable,
    gtk_window_set_deletable, gtk_window_get_deletable,
    gtk_window_is_active,
    gtk_window_set_modal, gtk_window_set_transient_for,
    gtk_widget_set_visible, gtk_widget_set_size_request,
    gtk_widget_get_width, gtk_widget_get_height,
    gtk_widget_set_opacity, gtk_widget_get_opacity,
    gtk_widget_add_controller,
    gtk_fixed_new,
    GtkScrolledWindow, GtkPolicyType,
    gtk_scrolled_window_new, gtk_scrolled_window_set_child,
    gtk_scrolled_window_set_policy,
    gtk_scrolled_window_set_propagate_natural_width,
    gtk_scrolled_window_set_propagate_natural_height,
    gtk_event_controller_key_new, gtk_event_controller_motion_new,
    gtk_event_controller_scroll_new, GtkEventControllerScroll,
    GtkEventControllerScrollFlags,
    gtk_gesture_click_new, gtk_gesture_single_set_button,
    gtk_gesture_single_get_current_button,
    gtk_event_controller_get_current_event_state,
    gdk_keyval_to_lower,
    g_main_loop_new, g_main_loop_run, g_main_loop_quit,
    g_signal_connect_data, g_signal_connect_data2, g_signal_connect_key,
    g_signal_connect_motion, g_signal_connect_click, g_signal_connect_scroll,
}
use core { WINDOWS, WindowSlot, GTK_INITED, LOOP_HOLDER }
use statusbar { linux_relayout_status_bar }
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
            // If this window is hosting a nested modal loop,
            // quit it before the slot evaporates so
            // `runNativeWindowModal` returns to its caller and
            // the surrounding `Window.show()` finishes.
            if slot.modalLoopRaw != 0 as i64 {
                g_main_loop_quit(slot.modalLoopRaw as GMainLoop)
            }
            let cb = slot.onClose
            cb()
            WINDOWS.delete(win)
        }
        if WINDOWS.size() == 0 as i64 {
            if LOOP_HOLDER.length > 0 { g_main_loop_quit(LOOP_HOLDER[0]) }
        }
    }

    /// `void(GObject* window, GParamSpec*, gpointer)` — GTK 4 updates
    /// the window's `default-width` / `default-height` properties as
    /// the user resizes a resizable window, so a `notify::` on either
    /// is the recommended resize hook. Both fire on a diagonal drag;
    /// each reads the current allocated size and forwards it, so the
    /// double-fire just delivers `onResize` twice with the same dims.
    pub fn linux_window_resize(win: i64, pspec: i64, data: i64) {
        if let some(slot) = WINDOWS.get(data) {
            // Measure the scrolled window (the window's content area
            // below the CSD title bar), NOT `win` — `win` includes the
            // title bar, so laying children out to it overshoots.
            let w = gtk_widget_get_width(slot.scroll as GtkWidget)
            let h = gtk_widget_get_height(slot.scroll as GtkWidget)
            // `notify::default-width/height` also fires from
            // `gtk_window_set_default_size` (called before the window is
            // realised, e.g. from the `width`/`height` setters), when
            // the allocation still reads back as 0. Forwarding that
            // would clobber the window's logical size to 0 and collapse
            // every flex child to zero width (buttons overlap). Only
            // forward a real, post-allocation size.
            if w > 0 as i32 && h > 0 as i32 {
                let cb = slot.onResize
                cb(w as f64, h as f64)
                // Pin the bottom-docked status bar (if any) so it
                // tracks the new content rectangle.
                linux_relayout_status_bar(data, w, h)
            }
        }
    }

    /// `void(GObject* window, GParamSpec*, gpointer)` — fired on
    /// every change of the corresponding GtkWindow property. Both
    /// the rising and falling edge fire `notify::`, so we compare
    /// the current value against the stashed `wasXxx` bit to pick
    /// between `onMaximize` / `onUnmaximize` (and full-screen).
    pub fn linux_window_maximized_notify(win: i64, pspec: i64, data: i64) {
        if let some(slot) = WINDOWS.get(data) {
            let now = gtk_window_is_maximized(slot.win) != 0 as i32
            if now != slot.wasMaximized {
                slot.wasMaximized = now
                if now { slot.onMaximize() } else { slot.onUnmaximize() }
            }
        }
    }
    pub fn linux_window_fullscreened_notify(win: i64, pspec: i64, data: i64) {
        if let some(slot) = WINDOWS.get(data) {
            let now = gtk_window_is_fullscreen(slot.win) != 0 as i32
            if now != slot.wasFullScreen {
                slot.wasFullScreen = now
                if now { slot.onEnterFullScreen() } else { slot.onLeaveFullScreen() }
            }
        }
    }
    pub fn linux_window_is_active_notify(win: i64, pspec: i64, data: i64) {
        if let some(slot) = WINDOWS.get(data) {
            let now = gtk_window_is_active(slot.win) != 0 as i32
            if now != slot.wasActive {
                slot.wasActive = now
                if now { slot.onFocus() } else { slot.onBlur() }
            }
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
        if let some(slot) = WINDOWS.get(data) {
            let rawBtn = gtk_gesture_single_get_current_button(gesture as GtkGestureSingle)
            if rawBtn == 3 as u32 {
                let st = gtk_event_controller_get_current_event_state(
                    gesture as GtkEventController
                )
                let ctx = slot.onContextMenu
                ctx(x, y, 2 as i32,
                    hasMod(st, MOD_CTRL), hasMod(st, MOD_SHIFT),
                    hasMod(st, MOD_ALT), hasMod(st, MOD_META))
            }
        }
    }
    pub fn linux_released(
        gesture: i64, n_press: i32, x: f64, y: f64, data: i64
    ) {
        dispatchClick(gesture, data, x, y, false)
    }
    pub fn linux_motion_enter(ctrl: i64, x: f64, y: f64, data: i64) {
        if let some(slot) = WINDOWS.get(data) {
            let cb = slot.onMouseEnter
            cb(x, y)
        }
    }
    pub fn linux_motion_leave(ctrl: i64, data: i64) {
        if let some(slot) = WINDOWS.get(data) {
            let cb = slot.onMouseLeave
            cb(0.0, 0.0)
        }
    }
    pub fn linux_scroll(ctrl: i64, dx: f64, dy: f64, data: i64): i32 {
        if let some(slot) = WINDOWS.get(data) {
            let st = gtk_event_controller_get_current_event_state(
                ctrl as GtkEventController
            )
            let cb = slot.onMouseWheel
            cb(dx, dy, 0.0, 0.0,
               hasMod(st, MOD_CTRL), hasMod(st, MOD_SHIFT),
               hasMod(st, MOD_ALT), hasMod(st, MOD_META))
        }
        0 as i32
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
    // A bare GtkFixed reports its children's bounding box as its
    // MINIMUM size, which becomes the window's minimum — so the window
    // can never be dragged smaller than its current content. Wrapping
    // it in a scrolled window with `external` policy draws no
    // scrollbars and (unlike `never`) does NOT propagate the fixed's
    // minimum up to the window, leaving it freely shrinkable. Children
    // re-flow to the content area on every resize, so nothing scrolls.
    let scroll = gtk_scrolled_window_new() as GtkScrolledWindow
    gtk_scrolled_window_set_policy(scroll, GtkPolicyType.external, GtkPolicyType.external)
    gtk_scrolled_window_set_propagate_natural_width(scroll, 0 as i32)
    gtk_scrolled_window_set_propagate_natural_height(scroll, 0 as i32)
    gtk_scrolled_window_set_child(scroll, fixed as GtkWidget)
    gtk_window_set_child(win, scroll as GtkWidget)

    let handle = win as i64
    WINDOWS[handle] = new WindowSlot(win, fixed, scroll)
    g_signal_connect_data(
        win as GObject,
        cstrFromString("destroy"),
        linux_window_destroy,
        0 as *void,
        0 as i64,
        0 as u32
    )

    // Resize — GTK 4 tracks the live size in default-width /
    // default-height, so notify on both drives onResize.
    g_signal_connect_data2(
        win as GObject, cstrFromString("notify::default-width"),
        linux_window_resize, handle as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data2(
        win as GObject, cstrFromString("notify::default-height"),
        linux_window_resize, handle as *void, 0 as i64, 0 as u32
    )

    // Window-state notify hooks — GTK 4 exposes `maximized` and
    // `fullscreened` boolean properties on GtkWindow; `notify::` on
    // either fires whenever the property flips. GTK 4 does NOT
    // expose a minimised property (Wayland-era simplification), so
    // `onMinimize` / `onRestore` are driven by the `minimize` /
    // `restore` action paths directly (see below).
    g_signal_connect_data2(
        win as GObject, cstrFromString("notify::maximized"),
        linux_window_maximized_notify, handle as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data2(
        win as GObject, cstrFromString("notify::fullscreened"),
        linux_window_fullscreened_notify, handle as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data2(
        win as GObject, cstrFromString("notify::is-active"),
        linux_window_is_active_notify, handle as *void, 0 as i64, 0 as u32
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
    g_signal_connect_data(
        motionCtrl as GObject, cstrFromString("enter"),
        linux_motion_enter, handle as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        motionCtrl as GObject, cstrFromString("leave"),
        linux_motion_leave, handle as *void, 0 as i64, 0 as u32
    )
    gtk_widget_add_controller(
        win as GtkWidget, motionCtrl as GtkEventController
    )

    // Scroll controller for onMouseWheel.
    let scrollCtrl = gtk_event_controller_scroll_new(
        GtkEventControllerScrollFlags.vertical
            | GtkEventControllerScrollFlags.horizontal
    ) as GtkEventControllerScroll
    g_signal_connect_scroll(
        scrollCtrl as GObject, cstrFromString("scroll"),
        linux_scroll, handle as *void, 0 as i64, 0 as u32
    )
    gtk_widget_add_controller(
        win as GtkWidget, scrollCtrl as GtkEventController
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

// ─── window state actions ──────────────────────────────────────

/// GTK 4 has no `is_minimized` query and no `notify::minimized`,
/// so we synthesise the `onMinimize` / `onRestore` events
/// synchronously from the action paths and track the boolean
/// ourselves via `slot.wasMinimized`.
pub fn minimizeNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_minimize(slot.win)
        if !slot.wasMinimized {
            slot.wasMinimized = true
            slot.onMinimize()
        }
    }
}
pub fn restoreNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_unminimize(slot.win)
        if slot.wasMinimized {
            slot.wasMinimized = false
            slot.onRestore()
        }
    }
}
pub fn maximizeNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_maximize(slot.win)
    }
}
pub fn unmaximizeNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_unmaximize(slot.win)
    }
}
pub fn setNativeWindowFullScreen(handle: i64, on: bool) {
    if let some(slot) = WINDOWS.get(handle) {
        if on { gtk_window_fullscreen(slot.win) }
        else  { gtk_window_unfullscreen(slot.win) }
    }
}

pub fn isNativeWindowMinimized(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        slot.wasMinimized
    } else {
        false
    }
}
pub fn isNativeWindowMaximized(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_is_maximized(slot.win) != 0 as i32
    } else {
        false
    }
}
pub fn isNativeWindowFullScreen(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_is_fullscreen(slot.win) != 0 as i32
    } else {
        false
    }
}

// ─── window-trait flags ────────────────────────────────────────

pub fn setNativeWindowResizable(handle: i64, on: bool) {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_set_resizable(slot.win, if on { 1 as i32 } else { 0 as i32 })
    }
}
pub fn isNativeWindowResizable(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_get_resizable(slot.win) != 0 as i32
    } else {
        true
    }
}

pub fn setNativeWindowClosable(handle: i64, on: bool) {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_set_deletable(slot.win, if on { 1 as i32 } else { 0 as i32 })
    }
}
pub fn isNativeWindowClosable(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_get_deletable(slot.win) != 0 as i32
    } else {
        true
    }
}

/// GTK4 has no `gtk_window_set_movable`; the property is a per-
/// slot flag we expose for API parity. The window manager always
/// honours its own move bindings, so this is informational only
/// on Linux.
pub fn setNativeWindowMovable(handle: i64, on: bool) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.isMovable = on
    }
}
pub fn isNativeWindowMovable(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        slot.isMovable
    } else {
        true
    }
}

// ─── focus / Z-order ───────────────────────────────────────────

pub fn focusNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_present(slot.win)
    }
}
/// GTK4 has no per-window blur primitive. `handle` is accepted
/// for API symmetry; the call is a no-op on Linux.
pub fn blurNativeWindow(handle: i64) {}

pub fn isNativeWindowFocused(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_is_active(slot.win) != 0 as i32
    } else {
        false
    }
}

/// GTK4 dropped `gtk_window_set_keep_above`. The flag is stored
/// on the slot for API parity; the window manager always owns
/// stacking on Linux.
pub fn setNativeWindowAlwaysOnTop(handle: i64, on: bool) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.isAlwaysOnTop = on
    }
}
pub fn isNativeWindowAlwaysOnTop(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        slot.isAlwaysOnTop
    } else {
        false
    }
}

pub fn moveTopNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_present(slot.win)
    }
}

// ─── geometry / position / bounds ──────────────────────────────
//
// GTK4 dropped per-window positioning (no `gtk_window_move`, no
// `gtk_window_get_position`) and never had a built-in maximum
// resize bound. We track everything on the slot for API parity
// and apply what GTK4 *can* honour:
//   * setContentSize / setBounds → `gtk_window_set_default_size`
//   * setMinimumSize             → `gtk_widget_set_size_request`
// Position and maximum size are recorded but visually no-op.

/// GTK4 has no `gtk_window_set_position(CENTER_ALWAYS)` and no
/// `gtk_window_move` — the WM owns placement. We do nothing
/// visible but keep the entry point for cross-platform parity.
pub fn centerNativeWindow(handle: i64) {}

pub fn setNativeWindowPosition(handle: i64, x: f64, y: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.posX = x
        slot.posY = y
    }
}
pub fn getNativeWindowX(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) { slot.posX } else { 0.0 }
}
pub fn getNativeWindowY(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) { slot.posY } else { 0.0 }
}

pub fn setNativeWindowBounds(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.posX = x
        slot.posY = y
        gtk_window_set_default_size(slot.win, w as i32, h as i32)
    }
}

pub fn setNativeWindowMinSize(handle: i64, w: f64, h: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.minW = w
        slot.minH = h
        gtk_widget_set_size_request(slot.win as GtkWidget, w as i32, h as i32)
    }
}
pub fn getNativeWindowMinWidth(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) { slot.minW } else { 0.0 }
}
pub fn getNativeWindowMinHeight(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) { slot.minH } else { 0.0 }
}
pub fn setNativeWindowMaxSize(handle: i64, w: f64, h: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.maxW = w
        slot.maxH = h
        slot.hasMaxSize = true
    }
}
pub fn getNativeWindowMaxWidth(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) {
        if slot.hasMaxSize { slot.maxW } else { 0.0 }
    } else {
        0.0
    }
}
pub fn getNativeWindowMaxHeight(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) {
        if slot.hasMaxSize { slot.maxH } else { 0.0 }
    } else {
        0.0
    }
}

/// GTK4 dropped `gtk_window_set_urgency_hint` and has no built-in
/// "demand user attention" hook. Public-API signature only for
/// cross-platform parity — call is a no-op on Linux.
pub fn flashNativeWindow(handle: i64, on: bool) {}

/// `setProgressBar` on Linux would route through the Unity
/// Launcher d-bus protocol (libunity), which isn't a portable
/// dependency. Public-API signature only for cross-platform
/// parity — call is a no-op on Linux.
pub fn setNativeWindowProgressBar(handle: i64, progress: f64) {}

// ─── parent / modal ────────────────────────────────────────────

pub fn setNativeWindowParent(handle: i64, parentHandle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        let parent: GtkWindow = if parentHandle != 0 as i64 {
            if let some(p) = WINDOWS.get(parentHandle) {
                p.win
            } else {
                0 as i64 as GtkWindow
            }
        } else {
            0 as i64 as GtkWindow
        }
        gtk_window_set_transient_for(slot.win, parent)
        slot.parentHandle = parentHandle
    }
}

pub fn setNativeWindowModal(handle: i64, on: bool) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.isModal = on
        gtk_window_set_modal(slot.win, if on { 1 as i32 } else { 0 as i32 })
    }
}
pub fn isNativeWindowModal(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) { slot.isModal } else { false }
}

/// Spin a nested GMainLoop until the modal window is closed.
/// GTK4's `set_modal(true)` already grabs input at the WM level;
/// the nested loop here is what makes `Window.show()` BLOCK
/// (matching the macOS / Win32 behaviour). `linux_window_destroy`
/// quits the loop on the close path.
pub fn runNativeWindowModal(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        let ml = g_main_loop_new(0 as *void, 0 as i32)
        slot.modalLoopRaw = ml as i64
        g_main_loop_run(ml)
        slot.modalLoopRaw = 0 as i64
    }
}

pub fn setNativeWindowOpacity(handle: i64, v: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_widget_set_opacity(slot.win as GtkWidget, v)
    }
}
pub fn getNativeWindowOpacity(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_widget_get_opacity(slot.win as GtkWidget)
    } else {
        1.0
    }
}

/// GTK 4 dropped `gtk_window_set_icon_from_file`; the supported
/// path goes through `gtk_window_set_icon_name`, which expects an
/// icon-theme name rather than a filesystem path. Round 2 may
/// wire `GdkTexture`-based custom icons through
/// `gtk_window_set_default_icon_name`. Stub for now.
pub fn setNativeWindowIcon(handle: i64, path: string) {}

} // @extern(C)
