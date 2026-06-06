/// GtkEntry — single-line text input.

use std.ffi { cstrFromString, stringFromCstr }
use gtk {
    g_signal_connect_click, g_signal_connect_motion,
    GtkWidget, GtkEntry, GtkFixed, GtkWindow, GObject,
    GtkEventController, GtkEventControllerFocus, GtkEventControllerMotion,
    GtkEventControllerKey, GtkEventControllerScroll, GtkEventControllerScrollFlags,
    GtkGestureSingle, GtkGestureClick,
    gtk_entry_new, gtk_editable_set_text, gtk_editable_get_text,
    gtk_editable_set_editable,
    gtk_editable_select_region, gtk_editable_get_selection_bounds,
    gtk_entry_set_placeholder_text, gtk_entry_set_max_length,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible,
    gtk_widget_set_tooltip_text,
    gtk_widget_grab_focus, gtk_widget_add_controller,
    gtk_event_controller_focus_new, gtk_event_controller_motion_new,
    gtk_event_controller_key_new,
    gtk_event_controller_scroll_new,
    gtk_event_controller_get_widget,
    gtk_event_controller_set_propagation_phase, GtkPropagationPhase,
    gtk_event_controller_get_current_event_state,
    gdk_keyval_to_lower,
    gtk_gesture_click_new,
    gtk_gesture_single_set_button, gtk_gesture_single_get_current_button,
    gtk_window_get_focus, gtk_window_set_focus,
    g_signal_connect_data, g_signal_connect_key, g_signal_connect_scroll,
}
use core { WINDOWS, resolveGtkParent, WindowSlot, noOpText, noOpClose, hasMod, gdkBtnToW3c, noOpEnterLeave, noOpWheel, noOpKey, noOpClickMod }
use window { keyvalToCode }

class EditSlot {
    pub entry: GtkEntry
    pub fixed: GtkFixed
    pub winHandle: i64
    pub onChange: fn(string)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onKeyDown: fn(string, bool, bool, bool, bool)
    pub onKeyUp:   fn(string, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub init(entry: GtkEntry, fixed: GtkFixed, winHandle: i64) {
        this.entry = entry
        this.fixed = fixed
        this.winHandle = winHandle
        this.onChange = noOpText
        this.onFocus  = noOpClose
        this.onBlur   = noOpClose
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseDown  = noOpClickMod
        this.onMouseUp    = noOpClickMod
        this.onContextMenu = noOpClickMod
        this.onKeyDown    = noOpKey
        this.onKeyUp      = noOpKey
        this.onMouseWheel = noOpWheel
    }
}


// GdkModifierType bits we need. Pulled out so the entry's gesture
// handlers can `& MOD_*` without going through the flags enum.
const MOD_SHIFT: u32 = 0x00000001 as u32
const MOD_CTRL:  u32 = 0x00000004 as u32
const MOD_ALT:   u32 = 0x00000008 as u32
const MOD_META:  u32 = 0x10000000 as u32



let EDITS = new Map<i64, EditSlot>()

@extern(C) {
    pub fn linux_entry_changed(entry: i64, data: i64) {
        let key = entry as i64
        if let some(slot) = EDITS.get(key) {
            let p = gtk_editable_get_text(slot.entry)
            let s = stringFromCstr(p)
            slot.onChange(s)
        }
    }
    /// `GtkEventControllerFocus.enter` /
    /// `GtkEventControllerFocus.leave` carry
    /// `(controller*, gpointer)`. Resolve the slot through the
    /// controller's attached widget, same trick the Button-side
    /// helpers use.
    pub fn linux_entry_focus_enter(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = EDITS.get(w as i64) {
            slot.onFocus()
        }
    }
    pub fn linux_entry_focus_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = EDITS.get(w as i64) {
            slot.onBlur()
        }
    }
    pub fn linux_entry_motion_enter(ctl: i64, x: f64, y: f64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = EDITS.get(w as i64) {
            slot.onMouseEnter(x, y)
        }
    }
    pub fn linux_entry_motion_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = EDITS.get(w as i64) {
            slot.onMouseLeave(0.0, 0.0)
        }
    }
    pub fn linux_entry_pressed(
        gesture: i64, nPress: i32, x: f64, y: f64, data: i64
    ) {
        let w = gtk_event_controller_get_widget(gesture as GtkEventController)
        if let some(slot) = EDITS.get(w as i64) {
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
    pub fn linux_entry_released(
        gesture: i64, nPress: i32, x: f64, y: f64, data: i64
    ) {
        let w = gtk_event_controller_get_widget(gesture as GtkEventController)
        if let some(slot) = EDITS.get(w as i64) {
            let btn = gtk_gesture_single_get_current_button(gesture as GtkGestureSingle)
            let st = gtk_event_controller_get_current_event_state(
                gesture as GtkEventController
            ) as u32
            slot.onMouseUp(x, y, gdkBtnToW3c(btn),
               hasMod(st, MOD_CTRL), hasMod(st, MOD_SHIFT),
               hasMod(st, MOD_ALT),  hasMod(st, MOD_META))
        }
    }
    pub fn linux_entry_key_pressed(
        ctrl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctrl as GtkEventController)
        if let some(slot) = EDITS.get(w as i64) {
            let code = keyvalToCode(gdk_keyval_to_lower(keyval))
            slot.onKeyDown(code,
               hasMod(state, MOD_CTRL), hasMod(state, MOD_SHIFT),
               hasMod(state, MOD_ALT),  hasMod(state, MOD_META))
        }
        0 as i32
    }
    pub fn linux_entry_key_released(
        ctrl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctrl as GtkEventController)
        if let some(slot) = EDITS.get(w as i64) {
            let code = keyvalToCode(gdk_keyval_to_lower(keyval))
            slot.onKeyUp(code,
               hasMod(state, MOD_CTRL), hasMod(state, MOD_SHIFT),
               hasMod(state, MOD_ALT),  hasMod(state, MOD_META))
        }
        0 as i32
    }
    pub fn linux_entry_scroll(
        ctrl: i64, dx: f64, dy: f64, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctrl as GtkEventController)
        if let some(slot) = EDITS.get(w as i64) {
            slot.onMouseWheel(dx, dy, 0.0, 0.0, false, false, false, false)
        }
        0 as i32
    }
}

@extern(C) {

pub fn createNativeEdit(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let entry = gtk_entry_new() as GtkEntry
    gtk_editable_set_text(entry, cstrFromString(text))
    gtk_widget_set_size_request(entry as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(
        entry as GtkWidget,
        if enabled { 1 as i32 } else { 0 as i32 }
    )
    gtk_widget_set_visible(
        entry as GtkWidget,
        if visible { 1 as i32 } else { 0 as i32 }
    )
    gtk_fixed_put(parent.fixed, entry as GtkWidget, x, y)

    g_signal_connect_data(
        entry as GObject,
        cstrFromString("changed"),
        linux_entry_changed,
        0 as *void, 0 as i64, 0 as u32
    )

    // Focus controller for onFocus / onBlur. GtkEntry routes
    // focus through the same GtkEventControllerFocus mechanism
    // GtkButton does — `enter` / `leave` map to the public Signals.
    let focusCtl = gtk_event_controller_focus_new()
    gtk_widget_add_controller(entry as GtkWidget, focusCtl)
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("enter"),
        linux_entry_focus_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("leave"),
        linux_entry_focus_leave, 0 as *void, 0 as i64, 0 as u32
    )

    // Motion controller for onMouseEnter / onMouseLeave.
    let motionCtl = gtk_event_controller_motion_new() as GtkEventControllerMotion
    gtk_widget_add_controller(entry as GtkWidget, motionCtl as GtkEventController)
    g_signal_connect_motion(
        motionCtl as GObject, cstrFromString("enter"),
        linux_entry_motion_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        motionCtl as GObject, cstrFromString("leave"),
        linux_entry_motion_leave, 0 as *void, 0 as i64, 0 as u32
    )

    // Click gesture for onMouseDown / onMouseUp + onContextMenu
    // (the latter fires inline on a right-button press). Set
    // button=0 to listen for every mouse button — the handler
    // reads the actual one back via the gesture.
    let clickGesture = gtk_gesture_click_new() as GtkGestureClick
    gtk_gesture_single_set_button(clickGesture as GtkGestureSingle, 0 as u32)
    gtk_widget_add_controller(entry as GtkWidget, clickGesture as GtkEventController)
    g_signal_connect_click(
        clickGesture as GObject, cstrFromString("pressed"),
        linux_entry_pressed, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_click(
        clickGesture as GObject, cstrFromString("released"),
        linux_entry_released, 0 as *void, 0 as i64, 0 as u32
    )

    // Key controller for onKeyDown / onKeyUp. Mirrors the
    // Button-side wiring; the controller is bound to the entry,
    // so events only flow while the entry has focus.
    let keyCtl = gtk_event_controller_key_new() as GtkEventControllerKey
    // Capture phase so the entry's controller sees key-pressed
    // before the inner GtkText consumes it — otherwise Enter (and
    // Tab) are swallowed by GtkText's activate / focus-move handling
    // and never reach onKeyDown. The handler returns 0 (propagate),
    // so the entry still processes the key normally afterwards.
    gtk_event_controller_set_propagation_phase(
        keyCtl as GtkEventController, GtkPropagationPhase.capture
    )
    gtk_widget_add_controller(entry as GtkWidget, keyCtl as GtkEventController)
    g_signal_connect_key(
        keyCtl as GObject, cstrFromString("key-pressed"),
        linux_entry_key_pressed, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_key(
        keyCtl as GObject, cstrFromString("key-released"),
        linux_entry_key_released, 0 as *void, 0 as i64, 0 as u32
    )

    // Scroll controller for onMouseWheel. Both axes enabled —
    // text fields rarely consume wheel events themselves so
    // returning 0 from the handler keeps the parent's default
    // (e.g. ScrolledWindow propagation) intact.
    let scrollCtl = gtk_event_controller_scroll_new(
        GtkEventControllerScrollFlags.vertical | GtkEventControllerScrollFlags.horizontal
    ) as GtkEventControllerScroll
    gtk_widget_add_controller(entry as GtkWidget, scrollCtl as GtkEventController)
    g_signal_connect_scroll(
        scrollCtl as GObject, cstrFromString("scroll"),
        linux_entry_scroll, 0 as *void, 0 as i64, 0 as u32
    )

    let handle = entry as i64
    EDITS[handle] = new EditSlot(entry, parent.fixed, windowHandle)
    handle
}

pub fn setOnEditChange(handle: i64, cb: fn(string)) {
    if let some(slot) = EDITS.get(handle) { slot.onChange = cb }
}
pub fn setOnEditFocus(handle: i64, cb: fn()) {
    if let some(slot) = EDITS.get(handle) { slot.onFocus = cb }
}
pub fn setOnEditBlur(handle: i64, cb: fn()) {
    if let some(slot) = EDITS.get(handle) { slot.onBlur = cb }
}
pub fn setOnEditMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = EDITS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnEditMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = EDITS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnEditMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = EDITS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnEditMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = EDITS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnEditContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = EDITS.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnEditKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = EDITS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnEditKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = EDITS.get(handle) { slot.onKeyUp = cb }
}
pub fn setOnEditMouseWheel(
    handle: i64,
    cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = EDITS.get(handle) { slot.onMouseWheel = cb }
}
pub fn selectAllEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        gtk_editable_select_region(slot.entry, 0 as i32, -1 as i32)
    }
}
pub fn setEditSelection(handle: i64, start: i64, end: i64) {
    if let some(slot) = EDITS.get(handle) {
        let s = if start < 0 as i64 { 0 as i64 } else { start }
        let e = if end < s { s } else { end }
        gtk_editable_select_region(slot.entry, s as i32, e as i32)
    }
}
pub fn getEditSelectionStart(handle: i64): i64 {
    if let some(slot) = EDITS.get(handle) {
        let start: i32 = 0 as i32
        let end: i32 = 0 as i32
        gtk_editable_get_selection_bounds(slot.entry, &start, &end)
        start as i64
    } else { 0 as i64 }
}
pub fn getEditSelectionEnd(handle: i64): i64 {
    if let some(slot) = EDITS.get(handle) {
        let start: i32 = 0 as i32
        let end: i32 = 0 as i32
        gtk_editable_get_selection_bounds(slot.entry, &start, &end)
        end as i64
    } else { 0 as i64 }
}
pub fn focusEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        let _ = gtk_widget_grab_focus(slot.entry as GtkWidget)
    }
}
pub fn blurEdit(handle: i64) {
    if let some(slot) = EDITS.get(handle) {
        // Only clear when this entry actually has focus, mirroring
        // the cocoa / win32 paths. Walk the parent WindowSlot to
        // find the GtkWindow that owns focus state.
        if let some(parent) = resolveGtkParent(slot.winHandle) {
            let cur = gtk_window_get_focus(parent.win)
            if cur as i64 == slot.entry as i64 {
                gtk_window_set_focus(parent.win, 0 as GtkWidget)
            }
        }
    }
}
pub fn setEditText(handle: i64, text: string) {
    if let some(slot) = EDITS.get(handle) {
        gtk_editable_set_text(slot.entry, cstrFromString(text))
    }
}
pub fn readEditText(handle: i64): string {
    match EDITS.get(handle) {
        some(slot) { stringFromCstr(gtk_editable_get_text(slot.entry)) }
        none       { "" }
    }
}
pub fn setEditFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = EDITS.get(handle) {
        gtk_widget_set_size_request(slot.entry as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.entry as GtkWidget, x, y)
    }
}
pub fn setEditEnabled(handle: i64, enabled: bool) {
    if let some(slot) = EDITS.get(handle) {
        gtk_widget_set_sensitive(
            slot.entry as GtkWidget,
            if enabled { 1 as i32 } else { 0 as i32 }
        )
    }
}
pub fn setEditVisible(handle: i64, visible: bool) {
    if let some(slot) = EDITS.get(handle) {
        gtk_widget_set_visible(
            slot.entry as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}
pub fn setEditPlaceholder(handle: i64, text: string) {
    if let some(slot) = EDITS.get(handle) {
        // Empty string → NULL clears the placeholder per GTK
        // convention (an empty cstring would still show a stub).
        if text == "" {
            gtk_entry_set_placeholder_text(slot.entry, 0 as *const char)
        } else {
            gtk_entry_set_placeholder_text(slot.entry, cstrFromString(text))
        }
    }
}
pub fn setEditReadOnly(handle: i64, readOnly: bool) {
    if let some(slot) = EDITS.get(handle) {
        gtk_editable_set_editable(
            slot.entry,
            if readOnly { 0 as i32 } else { 1 as i32 }
        )
    }
}
pub fn setEditMaxLength(handle: i64, max: i32) {
    if let some(slot) = EDITS.get(handle) {
        gtk_entry_set_max_length(slot.entry, max)
    }
}
pub fn setEditTooltip(handle: i64, text: string) {
    if let some(slot) = EDITS.get(handle) {
        // Empty string → pass NULL so GTK clears the tooltip
        // outright. Same logic as the Button-side helper.
        if text == "" {
            gtk_widget_set_tooltip_text(slot.entry as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.entry as GtkWidget, cstrFromString(text))
        }
    }
}

/// Clipboard / undo / scroll command surface — GTK 4 removed
/// `gtk_editable_copy_clipboard` and friends in favour of signal
/// emission, which needs `g_signal_emit_by_name` (not bound yet).
/// Linux ships these as stubs for Round 1; Round 2 wires them
/// through the signal API.
pub fn copyEdit(handle: i64) {}
pub fn cutEdit(handle: i64) {}
pub fn pasteEdit(handle: i64) {}
pub fn undoEdit(handle: i64) {}
pub fn redoEdit(handle: i64) {}
pub fn scrollEditToCaret(handle: i64) {}

} // @extern(C)
