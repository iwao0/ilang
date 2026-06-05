/// GtkSearchEntry wrapper. `search-changed` (debounced text
/// change) drives `onChange`; `activate` (Return) drives
/// `onSearch`; `stop-search` (Esc) drives `onClear`.

use std.ffi { cstrFromString, stringFromCstr }
use gtk {
    GtkWidget, GtkSearchEntry, GtkEditable, GtkFixed, GObject,
    GtkEventController, GtkEventControllerFocus, GtkEventControllerKey,
    gtk_search_entry_new, gtk_search_entry_set_placeholder_text,
    gtk_editable_set_text, gtk_editable_get_text,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_sensitive,
    gtk_widget_set_visible, gtk_widget_set_tooltip_text,
    gtk_widget_grab_focus, gtk_widget_add_controller,
    gtk_event_controller_focus_new,
    gtk_event_controller_key_new,
    gtk_event_controller_get_widget,
    gdk_keyval_to_lower,
    gtk_window_get_focus, gtk_window_set_focus,
    g_signal_connect_data, g_signal_connect_key,
}
use window { keyvalToCode }
use core { resolveGtkParent, noOpText, noOp1, noOpKey, hasMod }

class SearchFieldSlot {
    pub field: GtkSearchEntry
    pub fixed: GtkFixed
    pub winHandle: i64
    pub onChange: fn(string)
    pub onSearch: fn(string)
    pub onClear:  fn()
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub onKeyDown: fn(string, bool, bool, bool, bool)
    pub onKeyUp:   fn(string, bool, bool, bool, bool)
    pub init(field: GtkSearchEntry, fixed: GtkFixed, winHandle: i64) {
        this.field = field
        this.fixed = fixed
        this.winHandle = winHandle
        this.onChange = noOpText
        this.onSearch = noOpText
        this.onClear  = noOp1
        this.onFocus  = noOp1
        this.onBlur   = noOp1
        this.onKeyDown = noOpKey
        this.onKeyUp   = noOpKey
    }
}

const MOD_SHIFT: u32 = 0x00000001 as u32
const MOD_CTRL:  u32 = 0x00000004 as u32
const MOD_ALT:   u32 = 0x00000008 as u32
const MOD_META:  u32 = 0x10000000 as u32

let SEARCHFIELDS = new Map<i64, SearchFieldSlot>()

@extern(C) {
    pub fn linux_searchfield_search_changed(entry: i64, data: i64) {
        if let some(slot) = SEARCHFIELDS.get(entry) {
            slot.onChange(stringFromCstr(gtk_editable_get_text(slot.field as GtkEditable)))
        }
    }
    pub fn linux_searchfield_activate(entry: i64, data: i64) {
        if let some(slot) = SEARCHFIELDS.get(entry) {
            slot.onSearch(stringFromCstr(gtk_editable_get_text(slot.field as GtkEditable)))
        }
    }
    pub fn linux_searchfield_stop_search(entry: i64, data: i64) {
        if let some(slot) = SEARCHFIELDS.get(entry) {
            slot.onClear()
        }
    }
    pub fn linux_searchfield_focus_enter(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = SEARCHFIELDS.get(w as i64) {
            slot.onFocus()
        }
    }
    pub fn linux_searchfield_focus_leave(ctl: i64, data: i64) {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = SEARCHFIELDS.get(w as i64) {
            slot.onBlur()
        }
    }
    pub fn linux_searchfield_key_pressed(
        ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = SEARCHFIELDS.get(w as i64) {
            let code = keyvalToCode(gdk_keyval_to_lower(keyval))
            slot.onKeyDown(code,
               hasMod(state, MOD_CTRL), hasMod(state, MOD_SHIFT),
               hasMod(state, MOD_ALT),  hasMod(state, MOD_META))
        }
        0 as i32
    }
    pub fn linux_searchfield_key_released(
        ctl: i64, keyval: u32, keycode: u32, state: u32, data: i64
    ): i32 {
        let w = gtk_event_controller_get_widget(ctl as GtkEventController)
        if let some(slot) = SEARCHFIELDS.get(w as i64) {
            let code = keyvalToCode(gdk_keyval_to_lower(keyval))
            slot.onKeyUp(code,
               hasMod(state, MOD_CTRL), hasMod(state, MOD_SHIFT),
               hasMod(state, MOD_ALT),  hasMod(state, MOD_META))
        }
        0 as i32
    }
}

@extern(C) {

pub fn createNativeSearchField(
    windowHandle: i64,
    text: string, placeholder: string,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let se = gtk_search_entry_new() as GtkSearchEntry
    gtk_editable_set_text(se as GtkEditable, cstrFromString(text))
    if placeholder != "" {
        gtk_search_entry_set_placeholder_text(se, cstrFromString(placeholder))
    }
    gtk_widget_set_size_request(se as GtkWidget, width as i32, height as i32)
    gtk_widget_set_sensitive(se as GtkWidget, if enabled { 1 as i32 } else { 0 as i32 })
    gtk_widget_set_visible(se as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, se as GtkWidget, x, y)

    g_signal_connect_data(
        se as GObject, cstrFromString("search-changed"),
        linux_searchfield_search_changed, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        se as GObject, cstrFromString("activate"),
        linux_searchfield_activate, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        se as GObject, cstrFromString("stop-search"),
        linux_searchfield_stop_search, 0 as *void, 0 as i64, 0 as u32
    )

    let focusCtl = gtk_event_controller_focus_new()
    gtk_widget_add_controller(se as GtkWidget, focusCtl)
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("enter"),
        linux_searchfield_focus_enter, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_data(
        focusCtl as GObject, cstrFromString("leave"),
        linux_searchfield_focus_leave, 0 as *void, 0 as i64, 0 as u32
    )

    let keyCtl = gtk_event_controller_key_new() as GtkEventControllerKey
    gtk_widget_add_controller(se as GtkWidget, keyCtl as GtkEventController)
    g_signal_connect_key(
        keyCtl as GObject, cstrFromString("key-pressed"),
        linux_searchfield_key_pressed, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_key(
        keyCtl as GObject, cstrFromString("key-released"),
        linux_searchfield_key_released, 0 as *void, 0 as i64, 0 as u32
    )

    let handle = se as i64
    SEARCHFIELDS[handle] = new SearchFieldSlot(se, parent.fixed, windowHandle)
    handle
}

pub fn readSearchFieldText(handle: i64): string {
    match SEARCHFIELDS.get(handle) {
        some(slot) { stringFromCstr(gtk_editable_get_text(slot.field as GtkEditable)) }
        none       { "" }
    }
}
pub fn setSearchFieldText(handle: i64, text: string) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        gtk_editable_set_text(slot.field as GtkEditable, cstrFromString(text))
    }
}
pub fn setSearchFieldPlaceholder(handle: i64, text: string) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        if text == "" {
            gtk_search_entry_set_placeholder_text(slot.field, 0 as *const char)
        } else {
            gtk_search_entry_set_placeholder_text(slot.field, cstrFromString(text))
        }
    }
}
pub fn setSearchFieldFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        gtk_widget_set_size_request(slot.field as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.field as GtkWidget, x, y)
    }
}
pub fn setSearchFieldEnabled(handle: i64, enabled: bool) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        gtk_widget_set_sensitive(slot.field as GtkWidget,
            if enabled { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setSearchFieldVisible(handle: i64, visible: bool) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        gtk_widget_set_visible(slot.field as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setSearchFieldTooltip(handle: i64, text: string) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        if text == "" {
            gtk_widget_set_tooltip_text(slot.field as GtkWidget, 0 as *const char)
        } else {
            gtk_widget_set_tooltip_text(slot.field as GtkWidget, cstrFromString(text))
        }
    }
}
pub fn setOnSearchFieldChange(handle: i64, cb: fn(string)) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onChange = cb }
}
pub fn setOnSearchFieldSearch(handle: i64, cb: fn(string)) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onSearch = cb }
}
pub fn setOnSearchFieldClear(handle: i64, cb: fn()) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onClear = cb }
}
pub fn setOnSearchFieldFocus(handle: i64, cb: fn()) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onFocus = cb }
}
pub fn setOnSearchFieldBlur(handle: i64, cb: fn()) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onBlur = cb }
}
pub fn setOnSearchFieldKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnSearchFieldKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = SEARCHFIELDS.get(handle) { slot.onKeyUp = cb }
}
pub fn focusSearchField(handle: i64) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        let _ = gtk_widget_grab_focus(slot.field as GtkWidget)
    }
}
pub fn blurSearchField(handle: i64) {
    if let some(slot) = SEARCHFIELDS.get(handle) {
        if let some(parent) = resolveGtkParent(slot.winHandle) {
            let cur = gtk_window_get_focus(parent.win)
            if cur as i64 == slot.field as i64 {
                gtk_window_set_focus(parent.win, 0 as GtkWidget)
            }
        }
    }
}

} // @extern(C)
