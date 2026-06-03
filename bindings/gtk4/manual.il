/// Hand-written GTK 4 bindings.
///
/// Everything the GIR-driven generator can't (or shouldn't) emit
/// lands here:
/// - **Signal-connect variants.** GLib's `g_signal_connect_data`
///   takes a `GCallback` (a void(*)() typedef), so a single GIR
///   declaration can't pin the actual handler shape. libs/gui needs
///   five different shapes against the same C symbol; each is
///   declared as its own ilang fn with `@symbol("g_signal_connect_data")`.
/// - **Keyval enum.** The `GDK_KEY_*` keysyms are C `#define`s in
///   `gdk/gdkkeysyms.h`; GIR doesn't expose them.
/// - **GAction / GActionGroup / GActionMap.** GIR `<interface>`s
///   that the generator's first pass skips. Plus the few free
///   methods that take those interfaces as parameters
///   (`g_action_map_add_action`, `gtk_widget_insert_action_group`).
/// - **GMainLoop methods.** GIR puts these on a `<record>`, which
///   the generator currently drops methods from.
/// - **GtkComboBoxText family.** Deprecated since GTK 4.10 so the
///   generator skips it; libs/gui still relies on the dynamic
///   append/remove API.
/// - **GtkEditable text accessors.** Interface methods skipped by
///   the generator.
///
/// Items declared here are re-exported by the umbrella `gtk.il`
/// alongside the `generated/gen_*.il` files.

// Pull the few types we reference straight from the generated
// buckets — going through the umbrella would create a circular
// import (the umbrella `pub use`s this file too).
use generated.gobject { GObject }
use generated.glib { GMainLoop }
use generated.gtk { GtkWidget, GtkEntry, GtkDrawingArea }
use generated.gdk { GdkDisplay, GdkSurface, GdkFrameClock }

// ─── Gdk keyvals ─────────────────────────────────────────────
// Subset libs/gui maps to W3C `KeyboardEvent.code`. Values from
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
    // ─── GAction / GActionMap interfaces ──────────────────────

    @handle pub struct GAction {}
    @handle pub struct GActionGroup {}
    @handle pub struct GActionMap {}

    @lib pub fn g_action_map_add_action(map: GActionMap, action: GAction)

    @lib pub fn gtk_widget_insert_action_group(
        w: GtkWidget, name: *const char, group: GActionGroup
    )

    // ─── GMainLoop ────────────────────────────────────────────
    // The class itself is emitted by the generator (record handle);
    // its constructor / methods aren't, because the generator's
    // first pass walks `<class>` only.

    @lib pub fn g_main_loop_new(context: *void, is_running: i32): GMainLoop
    @lib pub fn g_main_loop_run(ml: GMainLoop)
    @lib pub fn g_main_loop_quit(ml: GMainLoop)

    // ─── GtkEditable interface methods ────────────────────────

    @lib pub fn gtk_editable_set_text(entry: GtkEntry, text: *const char)
    @lib pub fn gtk_editable_get_text(entry: GtkEntry): *const char
    /// Toggle whether the user can edit the contents. The widget
    /// stays focusable / hit-testable; only typed input is
    /// suppressed.
    @lib pub fn gtk_editable_set_editable(entry: GtkEntry, is_editable: i32)

    // ─── GtkComboBoxText (deprecated, generator skips) ────────

    @handle pub struct GtkComboBoxText {}
    @lib pub fn gtk_combo_box_text_new(): GtkComboBoxText
    @lib pub fn gtk_combo_box_text_append_text(
        c: GtkComboBoxText, text: *const char
    )
    @lib pub fn gtk_combo_box_text_remove_all(c: GtkComboBoxText)
    @lib pub fn gtk_combo_box_set_active(c: GtkComboBoxText, index: i32)
    @lib pub fn gtk_combo_box_get_active(c: GtkComboBoxText): i32

    // ─── GtkDrawingArea draw-func ─────────────────────────────
    // The GIR generator emits `GtkDrawingArea` and the content-size
    // accessors but drops `set_draw_func` because its second
    // argument is the `GtkDrawingAreaDrawFunc` callback typedef
    // (`void(GtkDrawingArea*, cairo_t* cr, int width, int height,
    // gpointer)`). The `cairo_t*` is passed as a bare `i64` so this
    // binding stays independent of the separate `cairo` package; the
    // callee casts it to `cairo.cairo_t`.
    @lib pub fn gtk_drawing_area_set_draw_func(
        area: GtkDrawingArea,
        draw_func: fn(i64, i64, i32, i32, i64),
        user_data: *void,
        destroy: i64
    )

    // ─── Signal-connect callback variants ─────────────────────
    // Same C symbol; each ilang name pins one callback shape.

    /// `void(GObject*, gpointer)` — 1-arg signals like `clicked`,
    /// `toggled`, `changed`, `value-changed`, `destroy`.
    @lib pub fn g_signal_connect_data(
        instance: GObject, signal_name: *const char,
        c_handler: fn(i64, i64),
        data: *void, destroy_data: i64, flags: u32
    ): u64

    /// `void(GObject*, T arg, gpointer)` — 2-arg signals like
    /// `row-selected`, `activate`.
    @symbol("g_signal_connect_data")
    @lib pub fn g_signal_connect_data2(
        instance: GObject, signal_name: *const char,
        c_handler: fn(i64, i64, i64),
        data: *void, destroy_data: i64, flags: u32
    ): u64

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

    /// `gboolean(GtkEventControllerScroll*, gdouble dx, gdouble dy,
    /// gpointer)` — `scroll` signal on `GtkEventControllerScroll`.
    /// Returning nonzero consumes the event.
    @symbol("g_signal_connect_data")
    @lib pub fn g_signal_connect_scroll(
        instance: GObject, signal_name: *const char,
        c_handler: fn(i64, f64, f64, i64): i32,
        data: *void, destroy_data: i64, flags: u32
    ): u64

    /// `void(GtkDrawingArea*, int width, int height, gpointer)` —
    /// `resize` signal on `GtkDrawingArea`. Fires whenever the
    /// drawing area's allocated size changes.
    @symbol("g_signal_connect_data")
    @lib pub fn g_signal_connect_resize(
        instance: GObject, signal_name: *const char,
        c_handler: fn(i64, i32, i32, i64),
        data: *void, destroy_data: i64, flags: u32
    ): u64

    // ─── GtkNative + GdkSurface accessors ─────────────────────
    // Needed by libs/gui/linux GpuView to extract the GdkSurface
    // backing a GtkWindow and, beneath it, the platform-native X11
    // Window / wl_surface that wgpu wraps. `GtkNative` is a GIR
    // <interface> the generator's first pass skips.
    @handle pub struct GtkNative {}

    @lib pub fn gtk_widget_get_native(widget: GtkWidget): GtkNative
    @lib pub fn gtk_native_get_surface(self: GtkNative): GdkSurface

    // ─── Frame-clock tick callback ────────────────────────────
    // `gboolean(GtkWidget*, GdkFrameClock*, gpointer)`. Return 1
    // (G_SOURCE_CONTINUE) to keep ticking, 0 (G_SOURCE_REMOVE) to
    // stop. Fires once per frame from the surface's GdkFrameClock —
    // GTK's CADisplayLink / DwmFlush equivalent.
    @lib pub fn gtk_widget_add_tick_callback(
        widget: GtkWidget,
        callback: fn(i64, i64, i64): i32,
        user_data: *void,
        notify: i64
    ): u32
    // `gtk_widget_remove_tick_callback` is already in generated/gtk.il.

    // ─── X11 backend extras ───────────────────────────────────
    // Exported from libgtk-4.so on systems built with the X11
    // backend. `gdk_x11_surface_get_xid` returns `Window`
    // (unsigned long, 8 bytes on Linux x86_64); cast to u64 for
    // `WGPUSurfaceSourceXlibWindow.window`. `gdk_x11_display_get_xdisplay`
    // returns the `Display*` — the value wgpu's surface source
    // wants verbatim.
    @lib pub fn gdk_x11_surface_get_xid(surface: GdkSurface): u64
    @lib pub fn gdk_x11_display_get_xdisplay(display: GdkDisplay): *void

    // ─── Wayland backend extras ───────────────────────────────
    // `gdk_wayland_surface_get_wl_surface` returns the `wl_surface*`;
    // `gdk_wayland_display_get_wl_display` returns the `wl_display*`.
    // Both go straight into `WGPUSurfaceSourceWaylandSurface`.
    @lib pub fn gdk_wayland_surface_get_wl_surface(surface: GdkSurface): *void
    @lib pub fn gdk_wayland_display_get_wl_display(display: GdkDisplay): *void
}
