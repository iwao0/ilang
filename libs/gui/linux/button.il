/// GtkButton wrapper. Mirrors libs/gui/cocoa/button.il and
/// libs/gui/win32/button.il.
///
/// Click handler dispatch:
/// - GTK 4's `clicked` signal callback shape is
///   `void(*)(GtkButton*, gpointer)`.
/// - One shared C handler (`linux_button_clicked`) resolves the
///   GtkButton pointer back to a ButtonSlot through `BUTTONS`.
/// - g_signal_connect_data takes the handler via `...`.

use std.ffi { cstrFromString }
use gtk {
    GtkWidget, GtkButton, GtkFixed, GtkWindow, GObject,
    GtkEventController, GtkEventControllerKey, GtkEventControllerScroll,
    GtkEventControllerScrollFlags,
    GtkGestureSingle, GtkGesture,
    gtk_button_new_with_label, gtk_button_set_label,
    gtk_fixed_put, gtk_fixed_move, gtk_fixed_remove,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible, gtk_widget_activate,
    gtk_widget_set_tooltip_text,
    gtk_widget_set_receives_default, gtk_window_set_default_widget,
    gtk_widget_grab_focus, gtk_widget_add_controller,
    gtk_window_get_focus, gtk_window_set_focus,
    gtk_event_controller_focus_new,
    gtk_event_controller_motion_new,
    gtk_event_controller_key_new,
    gtk_event_controller_scroll_new,
    gtk_event_controller_get_widget,
    gdk_keyval_to_lower,
    gtk_gesture_click_new,
    gtk_gesture_single_set_button,
    gtk_gesture_single_get_current_button,
    g_signal_connect_data, g_signal_connect_key, g_signal_connect_scroll,
}
use core { WINDOWS, noOpClick, noOpClose }
use window { keyvalToCode }

class ButtonSlot {
    pub btn:     GtkButton
    pub fixed:   GtkFixed
    pub win:     GtkWindow
    pub winHandle: i64
    pub x:       f64
    pub y:       f64
    pub width:   f64
    pub height:  f64
    pub onClick: fn(f64, f64, i32)
    pub onFocus: fn()
    pub onBlur:  fn()
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseDown:  fn(f64, f64, i32)
    pub onMouseUp:    fn(f64, f64, i32)
    pub onContextMenu: fn(f64, f64, i32)
    pub onKeyDown:    fn(string, bool, bool, bool, bool)
    pub onKeyUp:      fn(string, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub init(
        btn: GtkButton, fixed: GtkFixed, win: GtkWindow, winHandle: i64,
        x: f64, y: f64, w: f64, h: f64
    ) {
        this.btn = btn
        this.fixed = fixed
        this.win = win
        this.winHandle = winHandle
        this.x = x
        this.y = y
        this.width = w
        this.height = h
        this.onClick = noOpClick
        this.onFocus = noOpClose
        this.onBlur  = noOpClose
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseDown  = noOpClick
        this.onMouseUp    = noOpClick
        this.onContextMenu = noOpClick
        this.onKeyDown     = noOpKey
        this.onKeyUp       = noOpKey
        this.onMouseWheel  = noOpWheel
    }
}

fn noOpEnterLeave(x: f64, y: f64) {}
fn noOpKey(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn noOpWheel(dx: f64, dy: f64, x: f64, y: f64, ck: bool, sk: bool, ak: bool, mk: bool) {}

// GdkModifierType bits we need (same as window.il's). Pulled out
// here so the key handler can `& MOD_*` without bouncing through
// `gtk` constants.
const MOD_SHIFT: u32 = 0x00000001 as u32
const MOD_CTRL:  u32 = 0x00000004 as u32
const MOD_ALT:   u32 = 0x00000008 as u32
const MOD_META:  u32 = 0x10000000 as u32

fn hasMod(state: u32, mask: u32): bool { (state & mask) != 0 as u32 }

/// GDK button numbering → W3C `MouseEvent.button`. GDK uses 1
/// for primary, 2 for middle, 3 for secondary; the DOM uses
/// 0 / 1 / 2 respectively. Unknown buttons fall through to 0.
fn gdkBtnToW3c(gdk: u32): i32 {
    match gdk {
        1 as u32 { 0 as i32 }
        2 as u32 { 1 as i32 }
        3 as u32 { 2 as i32 }
        _        { 0 as i32 }
    }
}

let BUTTONS = new Map<i64, ButtonSlot>()

/// Window handle → current default-button handle. GTK's
/// `set_default_widget` auto-clears the previous default on its
/// own, but tracking the holder lets `setButtonDefault(_, false)`
/// know whether to call it with `NULL`.
let DEFAULT_BUTTONS = new Map<i64, i64>()

@extern(C) {
    pub fn linux_button_clicked(btn: i64, data: i64) {
        let key = btn as i64
        if let some(slot) = BUTTONS.get(key) {
            // Cocoa / Win32 surface the button's centre as the click
            // pos when the OS gives us none — match that.
            slot.onClick(
                slot.x + slot.width / 2.0,
                slot.y + slot.height / 2.0,
                0 as i32
            )
        }
    }
    /// `enter` / `leave` signals on `GtkEventControllerFocus` carry
    /// `(GtkEventControllerFocus*, gpointer)`. The controller knows
    /// which widget it's attached to via
    /// `gtk_event_controller_get_widget`; use that as the BUTTONS
    /// key (same key createNativeButton stored).
    pub fn linux_button_focus_enter(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = BUTTONS.get(w as i64) {
            let cb = slot.onFocus
            cb()
        }
    }
    pub fn linux_button_focus_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = BUTTONS.get(w as i64) {
            let cb = slot.onBlur
            cb()
        }
    }

    /// `GtkGestureClick.pressed` / `released` carry
    /// `(GtkGestureClick* gesture, gint n_press, gdouble x, gdouble y,
    /// gpointer user)`. The gesture is itself a `GtkEventController`,
    /// so the widget lookup mirrors the focus controllers.
    pub fn linux_button_pressed(
        gesture: i64, nPress: i32, x: f64, y: f64, data: i64
    ) {
        let w = gtk_event_controller_get_widget(gesture as GtkEventController)
        if let some(slot) = BUTTONS.get(w as i64) {
            let btn = gtk_gesture_single_get_current_button(gesture as GtkGestureSingle)
            let w3c = gdkBtnToW3c(btn)
            let cb = slot.onMouseDown
            cb(x, y, w3c)
            // Right-button press → also surface onContextMenu, the
            // moment GTK would otherwise begin showing the system
            // context menu. Keyboard-triggered menus (Menu key /
            // Shift+F10) aren't routed yet.
            if btn == 3 as u32 {
                let ctx = slot.onContextMenu
                ctx(x, y, w3c)
            }
        }
    }
    pub fn linux_button_released(
        gesture: i64, nPress: i32, x: f64, y: f64, data: i64
    ) {
        let w = gtk_event_controller_get_widget(gesture as GtkEventController)
        if let some(slot) = BUTTONS.get(w as i64) {
            let btn = gtk_gesture_single_get_current_button(gesture as GtkGestureSingle)
            let cb = slot.onMouseUp
            cb(x, y, gdkBtnToW3c(btn))
        }
    }
    /// `GtkEventControllerMotion.enter` carries
    /// `(controller*, gdouble x, gdouble y, gpointer user)`.
    pub fn linux_button_motion_enter(ctl: i64, x: f64, y: f64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = BUTTONS.get(w as i64) {
            let cb = slot.onMouseEnter
            cb(x, y)
        }
    }
    /// `GtkEventControllerMotion.leave` is positionless —
    /// `(controller*, gpointer user)`. Surface (0, 0) so the
    /// payload shape matches `onMouseEnter`.
    pub fn linux_button_motion_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = BUTTONS.get(w as i64) {
            let cb = slot.onMouseLeave
            cb(0.0, 0.0)
        }
    }
    /// `GtkEventControllerKey.key-pressed` /
    /// `GtkEventControllerKey.key-released` carry
    /// `(controller*, guint keyval, guint keycode,
    /// GdkModifierType state, gpointer)` and return a
    /// `gboolean`. We always return 0 so default activation
    /// (Space-press → click on a GtkButton) still runs.
    pub fn linux_button_key_pressed(
        ctrl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctrl as GtkEventController)
        if let some(slot) = BUTTONS.get(w as i64) {
            let code = keyvalToCode(gdk_keyval_to_lower(keyval))
            let cb = slot.onKeyDown
            cb(code,
               hasMod(state, MOD_CTRL), hasMod(state, MOD_SHIFT),
               hasMod(state, MOD_ALT),  hasMod(state, MOD_META))
        }
        0 as i32
    }
    pub fn linux_button_key_released(
        ctrl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctrl as GtkEventController)
        if let some(slot) = BUTTONS.get(w as i64) {
            let code = keyvalToCode(gdk_keyval_to_lower(keyval))
            let cb = slot.onKeyUp
            cb(code,
               hasMod(state, MOD_CTRL), hasMod(state, MOD_SHIFT),
               hasMod(state, MOD_ALT),  hasMod(state, MOD_META))
        }
        0 as i32
    }
    /// `GtkEventControllerScroll.scroll` carries
    /// `(controller*, gdouble dx, gdouble dy, gpointer)` and
    /// returns `gboolean`. We always return 0 so GTK's default
    /// scrolling (e.g. inside a parent ScrolledWindow) still
    /// runs. GtkEventControllerScroll doesn't carry the cursor
    /// position; pass `0.0` for x / y as the gpuview backend
    /// does.
    pub fn linux_button_scroll(
        ctrl: i64, dx: f64, dy: f64, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctrl as GtkEventController)
        if let some(slot) = BUTTONS.get(w as i64) {
            let cb = slot.onMouseWheel
            cb(dx, dy, 0.0, 0.0, false, false, false, false)
        }
        0 as i32
    }
}

@extern(C) {

pub fn createNativeButton(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool, isDefault: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let btn = gtk_button_new_with_label(cstrFromString(text)) as GtkButton
    gtk_widget_set_size_request(btn as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(
        btn as GtkWidget,
        if enabled { 1 as i32 } else { 0 as i32 }
    )
    gtk_widget_set_visible(
        btn as GtkWidget,
        if visible { 1 as i32 } else { 0 as i32 }
    )
    gtk_fixed_put(winSlot.fixed, btn as GtkWidget, x, y)

    g_signal_connect_data(
        btn as GObject,
        cstrFromString("clicked"),
        linux_button_clicked,
        0 as *void,
        0 as i64,
        0 as u32
    )

    // Attach a focus event controller so the widget's focus /
    // blur transitions surface to our linux_button_focus_*
    // handlers. The controller's `widget` association is set by
    // gtk_widget_add_controller; the handlers look up the
    // ButtonSlot through `gtk_event_controller_get_widget`.
    let focusCtl = gtk_event_controller_focus_new()
    gtk_widget_add_controller(btn as GtkWidget, focusCtl)
    g_signal_connect_data(
        focusCtl as GObject,
        cstrFromString("enter"),
        linux_button_focus_enter,
        0 as *void,
        0 as i64,
        0 as u32
    )
    g_signal_connect_data(
        focusCtl as GObject,
        cstrFromString("leave"),
        linux_button_focus_leave,
        0 as *void,
        0 as i64,
        0 as u32
    )

    // Pointer motion controller for onMouseEnter / onMouseLeave —
    // GTK4 dropped the GtkWidget enter / leave signals; the modern
    // way is a dedicated motion controller.
    let motionCtl = gtk_event_controller_motion_new()
    gtk_widget_add_controller(btn as GtkWidget, motionCtl)
    g_signal_connect_data(
        motionCtl as GObject,
        cstrFromString("enter"),
        linux_button_motion_enter,
        0 as *void,
        0 as i64,
        0 as u32
    )
    g_signal_connect_data(
        motionCtl as GObject,
        cstrFromString("leave"),
        linux_button_motion_leave,
        0 as *void,
        0 as i64,
        0 as u32
    )

    // Scroll controller for onMouseWheel. Enables vertical and
    // horizontal axes so the public Signal sees both. The
    // controller's signal returns 0 from our handler so parent
    // ScrolledWindow / range widgets still get their default
    // handling — buttons rarely consume wheel events outright.
    let scrollCtl = gtk_event_controller_scroll_new(
        GtkEventControllerScrollFlags.vertical | GtkEventControllerScrollFlags.horizontal
    ) as GtkEventControllerScroll
    gtk_widget_add_controller(btn as GtkWidget, scrollCtl as GtkEventController)
    g_signal_connect_scroll(
        scrollCtl as GObject,
        cstrFromString("scroll"),
        linux_button_scroll,
        0 as *void,
        0 as i64,
        0 as u32
    )

    // Keyboard controller for onKeyDown / onKeyUp. Attaches to
    // the button so GTK only routes events while the button has
    // focus — matches AppKit's responder-chain semantics.
    let keyCtl = gtk_event_controller_key_new() as GtkEventControllerKey
    gtk_widget_add_controller(btn as GtkWidget, keyCtl as GtkEventController)
    g_signal_connect_key(
        keyCtl as GObject,
        cstrFromString("key-pressed"),
        linux_button_key_pressed,
        0 as *void,
        0 as i64,
        0 as u32
    )
    g_signal_connect_key(
        keyCtl as GObject,
        cstrFromString("key-released"),
        linux_button_key_released,
        0 as *void,
        0 as i64,
        0 as u32
    )

    // Click gesture for onMouseDown / onMouseUp. `set_button(0)`
    // makes the gesture fire for any mouse button (default is
    // primary only); we map the GDK button id to the W3C
    // MouseEvent.button inside the handlers.
    let clickGesture = gtk_gesture_click_new()
    gtk_gesture_single_set_button(clickGesture as GtkGestureSingle, 0 as u32)
    gtk_widget_add_controller(btn as GtkWidget, clickGesture as GtkEventController)
    g_signal_connect_data(
        clickGesture as GObject,
        cstrFromString("pressed"),
        linux_button_pressed,
        0 as *void,
        0 as i64,
        0 as u32
    )
    g_signal_connect_data(
        clickGesture as GObject,
        cstrFromString("released"),
        linux_button_released,
        0 as *void,
        0 as i64,
        0 as u32
    )

    let handle = btn as i64
    BUTTONS[handle] = new ButtonSlot(
        btn, winSlot.fixed, winSlot.win, windowHandle,
        x, y, width, height
    )
    if isDefault { setButtonDefault(handle, true) }
    handle
}

pub fn setOnButtonClick(handle: i64, cb: fn(f64, f64, i32)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onClick = cb }
}

pub fn setButtonText(handle: i64, text: string) {
    if let some(slot) = BUTTONS.get(handle) {
        gtk_button_set_label(slot.btn, cstrFromString(text))
    }
}

pub fn setButtonFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = BUTTONS.get(handle) {
        slot.x = x
        slot.y = y
        slot.width = width
        slot.height = height
        gtk_widget_set_size_request(slot.btn as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.btn as GtkWidget, x, y)
    }
}

pub fn setButtonEnabled(handle: i64, enabled: bool) {
    if let some(slot) = BUTTONS.get(handle) {
        gtk_widget_set_sensitive(
            slot.btn as GtkWidget,
            if enabled { 1 as i32 } else { 0 as i32 }
        )
    }
}

pub fn setButtonVisible(handle: i64, visible: bool) {
    if let some(slot) = BUTTONS.get(handle) {
        gtk_widget_set_visible(
            slot.btn as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}
pub fn clickButton(handle: i64) {
    if let some(slot) = BUTTONS.get(handle) {
        // `gtk_widget_activate` fires the "clicked" signal on
        // GtkButton, which our `linux_button_clicked` handler
        // routes back to `onClick`.
        gtk_widget_activate(slot.btn as GtkWidget)
    }
}
pub fn setOnButtonFocus(handle: i64, cb: fn()) {
    if let some(slot) = BUTTONS.get(handle) { slot.onFocus = cb }
}
pub fn setOnButtonBlur(handle: i64, cb: fn()) {
    if let some(slot) = BUTTONS.get(handle) { slot.onBlur = cb }
}
pub fn setOnButtonMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnButtonMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnButtonMouseDown(handle: i64, cb: fn(f64, f64, i32)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnButtonMouseUp(handle: i64, cb: fn(f64, f64, i32)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnButtonContextMenu(handle: i64, cb: fn(f64, f64, i32)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnButtonKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnButtonKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onKeyUp = cb }
}
pub fn setOnButtonMouseWheel(
    handle: i64,
    cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = BUTTONS.get(handle) { slot.onMouseWheel = cb }
}
pub fn setButtonTooltip(handle: i64, text: string) {
    if let some(slot) = BUTTONS.get(handle) {
        // Empty string → pass NULL so GTK clears the tooltip
        // outright (`gtk_widget_set_tooltip_text` documents NULL
        // as the "remove tooltip" path; an empty cstring would
        // still register an empty tooltip).
        if text == "" {
            gtk_widget_set_tooltip_text(slot.btn as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.btn as GtkWidget, cstrFromString(text))
        }
    }
}
pub fn focusButton(handle: i64) {
    if let some(slot) = BUTTONS.get(handle) {
        // `grab_focus` triggers the focus controller's `enter`
        // signal on this widget (and `leave` on the previous
        // holder), which routes through our linux_button_focus_*
        // handlers back to `onFocus` / `onBlur`.
        let _ = gtk_widget_grab_focus(slot.btn as GtkWidget)
    }
}
pub fn blurButton(handle: i64) {
    if let some(slot) = BUTTONS.get(handle) {
        // Web `blur()` only acts when this widget is the current
        // focus holder; otherwise we'd steal focus from whatever
        // sibling actually has it. Clear by passing NULL to the
        // window's focus setter — GTK then leaves no widget focused
        // and our focus controller's `leave` fires onBlur.
        let cur = gtk_window_get_focus(slot.win)
        if cur as i64 == slot.btn as i64 {
            gtk_window_set_focus(slot.win, 0 as GtkWidget)
        }
    }
}
pub fn setButtonDefault(handle: i64, isDefault: bool) {
    if let some(slot) = BUTTONS.get(handle) {
        if isDefault {
            gtk_widget_set_receives_default(slot.btn as GtkWidget, 1 as i32)
            gtk_window_set_default_widget(slot.win, slot.btn as GtkWidget)
            DEFAULT_BUTTONS[slot.winHandle] = handle
        } else {
            gtk_widget_set_receives_default(slot.btn as GtkWidget, 0 as i32)
            // Clear only if *this* button is still the window's
            // default; otherwise another button already took over.
            if let some(cur) = DEFAULT_BUTTONS.get(slot.winHandle) {
                if cur == handle {
                    gtk_window_set_default_widget(slot.win, 0 as GtkWidget)
                    DEFAULT_BUTTONS.delete(slot.winHandle)
                }
            }
        }
    }
}

} // @extern(C)
