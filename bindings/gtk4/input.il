/// GTK 4 input handling — event controllers + the GdkKeyval table
/// libs/gui needs to drive its key / mouse callbacks.
///
/// GTK 4 dropped the old `key-press-event` signals on
/// `GtkWidget`. Instead you attach `GtkEventController*` subclass
/// instances to a widget; each controller emits its own signals
/// (`key-pressed` / `key-released` on `GtkEventControllerKey`,
/// `pressed` / `released` / `motion` on `GtkGestureClick` /
/// `GtkEventControllerMotion`).

use core { GtkWidget, GObject }

// ─── Gdk modifier mask bits ──────────────────────────────────
// Mirrors GDK 4's `GdkModifierType` — the `state` argument the
// key / button signals pass alongside the keyval.
@flags
pub enum GdkModifierType: u32 {
    shift   = 0x00000001
    lock    = 0x00000002
    control = 0x00000004
    alt     = 0x00000008
    button1 = 0x00000100
    button2 = 0x00000200
    button3 = 0x00000400
    button4 = 0x00000800
    button5 = 0x00001000
    /// Cmd on macOS / Win key on Linux. Cocoa calls this `command`,
    /// win32 calls it `meta`; libs/gui's event signal also uses
    /// "meta" so authors don't have to know which they're on.
    meta    = 0x10000000
}

// ─── Gdk keyvals ─────────────────────────────────────────────
// The subset libs/gui maps to W3C `KeyboardEvent.code`. Add more
// variants here as new key events surface. Values from
// `<gdk/gdkkeysyms.h>`; letters are the lowercase form because
// callers always normalise via `gdk_keyval_to_lower` first.
pub enum Keyval: u32 {
    unknown    = 0x0000
    keyA       = 0x0061
    keyB       = 0x0062
    keyC       = 0x0063
    keyD       = 0x0064
    keyE       = 0x0065
    keyF       = 0x0066
    keyG       = 0x0067
    keyH       = 0x0068
    keyI       = 0x0069
    keyJ       = 0x006a
    keyK       = 0x006b
    keyL       = 0x006c
    keyM       = 0x006d
    keyN       = 0x006e
    keyO       = 0x006f
    keyP       = 0x0070
    keyQ       = 0x0071
    keyR       = 0x0072
    keyS       = 0x0073
    keyT       = 0x0074
    keyU       = 0x0075
    keyV       = 0x0076
    keyW       = 0x0077
    keyX       = 0x0078
    keyY       = 0x0079
    keyZ       = 0x007a
    digit0     = 0x0030
    digit1     = 0x0031
    digit2     = 0x0032
    digit3     = 0x0033
    digit4     = 0x0034
    digit5     = 0x0035
    digit6     = 0x0036
    digit7     = 0x0037
    digit8     = 0x0038
    digit9     = 0x0039
    space      = 0x0020
    enter      = 0xff0d
    escape     = 0xff1b
    backspace  = 0xff08
    tab        = 0xff09
    arrowLeft  = 0xff51
    arrowUp    = 0xff52
    arrowRight = 0xff53
    arrowDown  = 0xff54
}

@extern(C, "gtk-4") {
    @handle pub struct GtkEventController {}
    @handle pub struct GtkEventControllerKey {}
    @handle pub struct GtkEventControllerMotion {}
    @handle pub struct GtkGestureClick {}

    @lib pub fn gtk_event_controller_key_new(): GtkEventControllerKey
    @lib pub fn gtk_event_controller_motion_new(): GtkEventControllerMotion
    @lib pub fn gtk_gesture_click_new(): GtkGestureClick
    /// `button`: 0 = any button, 1 = primary, 2 = middle, 3 = right.
    @lib pub fn gtk_gesture_single_set_button(
        g: GtkGestureClick, button: u32
    )

    /// Attach the controller to a widget so its signals start
    /// firing.
    @lib pub fn gtk_widget_add_controller(
        w: GtkWidget, controller: GtkEventController
    )

    // ─── Shortcut controller (menu accelerators) ─────────────

    /// `GtkShortcutController` dispatches keyboard shortcuts to
    /// `GtkShortcut` actions. Attach to a window via
    /// `gtk_widget_add_controller` and add shortcuts via
    /// `gtk_shortcut_controller_add_shortcut`.
    @handle pub struct GtkShortcutController {}
    @handle pub struct GtkShortcut {}
    @handle pub struct GtkShortcutTrigger {}
    @handle pub struct GtkShortcutAction {}

    @lib pub fn gtk_shortcut_controller_new(): GtkShortcutController
    /// Scope: 0 = LOCAL (only when a child has focus), 1 = MANAGED,
    /// 2 = GLOBAL. libs/gui uses GLOBAL so menu accelerators fire
    /// regardless of which widget owns focus.
    @lib pub fn gtk_shortcut_controller_set_scope(
        c: GtkShortcutController, scope: i32
    )
    @lib pub fn gtk_shortcut_controller_add_shortcut(
        c: GtkShortcutController, shortcut: GtkShortcut
    )
    /// `gtk_shortcut_new` takes ownership of both arguments.
    @lib pub fn gtk_shortcut_new(
        trigger: GtkShortcutTrigger, action: GtkShortcutAction
    ): GtkShortcut
    /// Parse a string like `"<Primary>n"` (Primary = Cmd on macOS,
    /// Ctrl elsewhere). Returns NULL on parse failure.
    @lib pub fn gtk_shortcut_trigger_parse_string(
        s: *const char
    ): GtkShortcutTrigger
    /// `gtk_named_action_new("app.menu_3")` builds a
    /// `GtkShortcutAction` that activates the named action when the
    /// trigger fires.
    @lib pub fn gtk_named_action_new(name: *const char): GtkShortcutAction

    // ─── Signal connect variants for event controllers ───────
    // Same C symbol as `g_signal_connect_data`; distinct ilang names
    // pin the callback signature for each signal shape.

    /// `void(GtkEventControllerMotion*, gdouble x, gdouble y, gpointer)`.
    @symbol("g_signal_connect_data")
    @lib pub fn g_signal_connect_motion(
        instance: GObject, signal_name: *const char,
        c_handler: fn(i64, f64, f64, i64),
        data: *void, destroy_data: i64, flags: u32
    ): u64

    /// `void(GtkGestureClick*, gint n_press, gdouble x, gdouble y, gpointer)`.
    @symbol("g_signal_connect_data")
    @lib pub fn g_signal_connect_click(
        instance: GObject, signal_name: *const char,
        c_handler: fn(i64, i32, f64, f64, i64),
        data: *void, destroy_data: i64, flags: u32
    ): u64

    /// `gboolean(GtkEventControllerKey*, guint keyval, guint keycode,
    /// GdkModifierType state, gpointer)`. Returning nonzero consumes
    /// the event; libs/gui returns zero so menubars and entries still
    /// see their accelerators.
    @symbol("g_signal_connect_data")
    @lib pub fn g_signal_connect_key(
        instance: GObject, signal_name: *const char,
        c_handler: fn(i64, u32, u32, u32, i64): i32,
        data: *void, destroy_data: i64, flags: u32
    ): u64

    // ─── Gesture / event-controller state queries ────────────

    /// Returns the button that triggered the current sequence
    /// (1 = primary, 2 = middle, 3 = right). Useful inside a
    /// `GtkGestureClick` `pressed` / `released` handler.
    @lib pub fn gtk_gesture_single_get_current_button(
        g: GtkGestureClick
    ): u32

    /// Modifier mask for the in-progress event. `GdkModifierType` —
    /// bit-test against `GdkModifierType.shift` / `.control` / etc.
    @lib pub fn gtk_event_controller_get_current_event_state(
        c: GtkEventController
    ): u32

    /// Normalise to lowercase so shift state doesn't change the
    /// reported key code (Shift+a still yields keyval `a`).
    @lib pub fn gdk_keyval_to_lower(keyval: u32): u32
}
