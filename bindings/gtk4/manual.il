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
use generated.glib { GMainLoop, GVariant }
use generated.gtk { GtkWidget, GtkEntry, GtkDrawingArea, GtkWindow,
                    GtkAlertDialog, GtkFileDialog }
use generated.gio { GCancellable }
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

    // ─── GVariant constructors / accessors ────────────────────
    // Skipped by the generator because they take / return raw
    // C primitive shapes (gboolean, const gchar*) the GIR pass
    // doesn't widen automatically.

    @lib pub fn g_variant_new_boolean(value: i32): GVariant
    @lib pub fn g_variant_new_string(value: *const char): GVariant
    @lib pub fn g_variant_get_boolean(value: GVariant): i32
    @lib pub fn g_variant_get_string(value: GVariant, length: *u64): *const char

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
    /// Select `[start, end)`; `end = -1` extends to the end of
    /// the text. Pass `start == end` to collapse the selection
    /// to a caret at `start`.
    @lib pub fn gtk_editable_select_region(entry: GtkEntry, start: i32, end: i32)
    /// Fill `*start` / `*end` with the current selection bounds.
    /// When no selection exists, both receive the caret position.
    /// Returns non-zero when there's a real selection.
    @lib pub fn gtk_editable_get_selection_bounds(
        entry: GtkEntry, start: *i32, end: *i32
    ): i32

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

    // ─── GtkOrientable interface ──────────────────────────────
    // GIR `<interface>` skipped by the generator. The setter
    // accepts a GtkProgressBar (or any GtkOrientable subclass);
    // we type the argument as GtkWidget so any caller-supplied
    // widget pointer can be passed through unchanged.
    @lib pub fn gtk_orientable_set_orientation(
        orientable: GtkWidget, orientation: i32
    )

    // ─── GtkAlertDialog / GtkFileDialog async API ─────────────
    // GIR skips these because they go through
    // `GAsyncReadyCallback` (a function-pointer typedef) and
    // return GObject-rooted result handles GIR can't widen.
    // Manual binding here gives libs/gui dialogs a callback
    // shape that mirrors the cocoa sheet flow.

    @handle pub struct GFile {}
    @handle pub struct GAsyncResult {}

    /// `GtkAlertDialog` constructor + chooser methods. The
    /// trailing `*va_list` overload of `_new` isn't exposed —
    /// callers set the message via `set_message` instead.
    @lib pub fn gtk_alert_dialog_new(message: *const char): GtkAlertDialog
    /// `void(GObject* source, GAsyncResult* result, gpointer)`.
    @lib pub fn gtk_alert_dialog_choose(
        self: GtkAlertDialog,
        parent: GtkWindow,
        cancellable: GCancellable,
        callback: fn(i64, i64, i64),
        user_data: *void
    )
    /// Returns the 0-indexed button label that was clicked, or
    /// `-1` on error. `error_out` may be NULL (`0 as *void`).
    @lib pub fn gtk_alert_dialog_choose_finish(
        self: GtkAlertDialog, result: GAsyncResult, error_out: *void
    ): i32

    @lib pub fn gtk_file_dialog_new(): GtkFileDialog
    @lib pub fn gtk_file_dialog_set_title(self: GtkFileDialog, title: *const char)
    @lib pub fn gtk_file_dialog_set_initial_name(
        self: GtkFileDialog, name: *const char
    )
    @lib pub fn gtk_file_dialog_open(
        self: GtkFileDialog,
        parent: GtkWindow,
        cancellable: GCancellable,
        callback: fn(i64, i64, i64),
        user_data: *void
    )
    @lib pub fn gtk_file_dialog_open_finish(
        self: GtkFileDialog, result: GAsyncResult, error_out: *void
    ): GFile
    @lib pub fn gtk_file_dialog_save(
        self: GtkFileDialog,
        parent: GtkWindow,
        cancellable: GCancellable,
        callback: fn(i64, i64, i64),
        user_data: *void
    )
    @lib pub fn gtk_file_dialog_save_finish(
        self: GtkFileDialog, result: GAsyncResult, error_out: *void
    ): GFile

    /// `GFile` → filesystem path (Linux: UTF-8). Returns the path
    /// owned by the caller; the `g_free` cost is acceptable.
    @lib pub fn g_file_get_path(file: GFile): *const char

    // ─── GtkAlertDialog button-label override ─────────────────
    // The generated `gtk_alert_dialog_set_buttons` takes a single
    // `*const char` because GIR collapses the `const char**` C
    // signature. Re-bind it here with the correct pointer-of-
    // pointer shape: ilang callers fill an `i64[]` with
    // `cstrFromString` results and a trailing `0` sentinel, then
    // pass it through this symbol-aliased declaration.
    @symbol("gtk_alert_dialog_set_buttons")
    @lib pub fn gtk_alert_dialog_set_buttons_array(
        self: GtkAlertDialog, labels: *const i64
    )

    // ─── GtkColorDialog / GtkFontDialog async API ─────────────
    // GTK 4.10+ chooser dialogs. Same async flow as
    // GtkAlertDialog: kick off `_choose_*`, the
    // `GAsyncReadyCallback` fires when the user dismisses the
    // dialog, drain the result through `_choose_*_finish`.

    @lib pub fn gtk_color_dialog_choose_rgba(
        self: GtkColorDialog,
        parent: GtkWindow,
        initial_color: *const f32,        // GdkRGBA { r, g, b, a } as 4×f32
        cancellable: GCancellable,
        callback: fn(i64, i64, i64),
        user_data: *void
    )
    /// Returns a freshly-allocated `GdkRGBA*` (or NULL on cancel /
    /// error). The 4 f32 components live at offsets 0/4/8/12;
    /// caller frees via `gdk_rgba_free`.
    @lib pub fn gtk_color_dialog_choose_rgba_finish(
        self: GtkColorDialog, result: GAsyncResult, error_out: *void
    ): *void
    @lib pub fn gdk_rgba_free(rgba: *void)

    @handle pub struct PangoFontDescription {}

    @lib pub fn gtk_font_dialog_choose_font(
        self: GtkFontDialog,
        parent: GtkWindow,
        initial_value: PangoFontDescription,
        cancellable: GCancellable,
        callback: fn(i64, i64, i64),
        user_data: *void
    )
    /// Returns a freshly-allocated `PangoFontDescription*` (or
    /// NULL on cancel / error). Caller frees via
    /// `pango_font_description_free`.
    @lib pub fn gtk_font_dialog_choose_font_finish(
        self: GtkFontDialog, result: GAsyncResult, error_out: *void
    ): PangoFontDescription
    @lib pub fn pango_font_description_from_string(s: *const char): PangoFontDescription
    @lib pub fn pango_font_description_free(desc: PangoFontDescription)
    /// Family string — owned by `desc`; copy before freeing.
    @lib pub fn pango_font_description_get_family(desc: PangoFontDescription): *const char
    /// Size in pango units: `points × PANGO_SCALE (= 1024)`.
    @lib pub fn pango_font_description_get_size(desc: PangoFontDescription): i32
    /// `1` if `get_size` is in pixels (absolute), `0` if in points.
    @lib pub fn pango_font_description_get_size_is_absolute(desc: PangoFontDescription): i32
    /// CSS-style weight (100..1000); 400=Normal, 700=Bold.
    @lib pub fn pango_font_description_get_weight(desc: PangoFontDescription): i32
    /// `PangoStyle`: 0=Normal, 1=Oblique, 2=Italic.
    @lib pub fn pango_font_description_get_style(desc: PangoFontDescription): i32

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

    // ─── GListModel / selection models / column-view constructors ─
    // GTK4 list / tree widgets all sit on top of `GListModel`. The
    // generator emits the interface class struct but not the
    // interface handle itself, so we declare it here. Same for the
    // selection-model interface that `GtkColumnView` / `GtkListView`
    // bind to.
    @handle pub struct GListModel {}
    @handle pub struct GtkSelectionModel {}

    /// `gtk_tree_list_model_new(root, passthrough, autoexpand,
    ///   create_func, user_data, user_destroy): GtkTreeListModel`.
    /// `root` is the top-level `GListModel`; `create_func` is
    /// invoked lazily on each item to produce that item's children
    /// list (or `0` for leaves). The callback shape is
    /// `GListModel*(GObject *item, gpointer user_data)`.
    @lib pub fn gtk_tree_list_model_new(
        root: GListModel, passthrough: i32, autoexpand: i32,
        create_func: fn(i64, i64): i64,
        user_data: *void, user_destroy: i64
    ): GtkTreeListModel

    /// `gtk_single_selection_new(model): GtkSingleSelection` —
    /// the canonical "0 or 1 row selected" model wrapper.
    @lib pub fn gtk_single_selection_new(model: GListModel): GtkSingleSelection
    @lib pub fn gtk_single_selection_get_selected(self: GtkSingleSelection): u32
    @lib pub fn gtk_single_selection_set_selected(self: GtkSingleSelection, position: u32)
    @lib pub fn gtk_single_selection_get_selected_item(self: GtkSingleSelection): GObject

    /// `gtk_column_view_new(selection): GtkWidget`. Pass the
    /// selection model that wraps your data model. The view binds
    /// rows via columns + factories.
    @lib pub fn gtk_column_view_new(model: GtkSelectionModel): GtkWidget
    @lib pub fn gtk_column_view_set_model(self: GtkColumnView, model: GtkSelectionModel)

    /// `gtk_list_view_new(model, factory): GtkWidget` — single-
    /// column counterpart to `GtkColumnView`. Suitable when the
    /// tree only needs one visual column.
    @lib pub fn gtk_list_view_new(model: GtkSelectionModel, factory: GtkListItemFactory): GtkWidget
    @lib pub fn gtk_list_view_set_model(self: GtkListView, model: GtkSelectionModel)
    @lib pub fn gtk_list_view_set_factory(self: GtkListView, factory: GtkListItemFactory)

    /// `GtkSignalListItemFactory` setup / bind / unbind / teardown
    /// signal shape: `void(GtkSignalListItemFactory*, GtkListItem*,
    /// gpointer)`. Same C signal-connect as the 2-arg variant.
    @symbol("g_signal_connect_data")
    @lib pub fn g_signal_connect_factory(
        instance: GObject, signal_name: *const char,
        c_handler: fn(i64, i64, i64),
        data: *void, destroy_data: i64, flags: u32
    ): u64

    /// `notify::selected` on `GtkSingleSelection` — generic
    /// `void(GObject*, GParamSpec*, gpointer)`. We thread it
    /// through the same shape as our 2-arg connect; the
    /// `GParamSpec*` arrives in the second slot.
    @symbol("g_signal_connect_data")
    @lib pub fn g_signal_connect_notify(
        instance: GObject, signal_name: *const char,
        c_handler: fn(i64, i64, i64),
        data: *void, destroy_data: i64, flags: u32
    ): u64
}
