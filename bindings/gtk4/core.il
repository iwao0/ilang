/// Application + Window + Widget basics, plus the GLib / GObject
/// helpers libs/gui needs for signal binding and refcount handling.

@extern(C, "gtk-4") {
    /// Every GTK widget is a `GtkWidget*` at the C ABI level.
    /// Concrete subclasses (`GtkButton`, `GtkWindow`, ...) cast to
    /// `GtkWidget*` freely — we keep them as distinct `@handle`
    /// types so the ilang type checker still differentiates them.
    @handle pub struct GtkWidget {}
    @handle pub struct GtkWindow {}
    @handle pub struct GtkApplication {}
    @handle pub struct GtkApplicationWindow {}

    /// `GApplication` — `GtkApplication`'s parent. The `g_*`
    /// lifecycle entry points (`g_application_run`,
    /// `g_application_quit`) take this base type. Cast a
    /// `GtkApplication` to `GApplication` at the call site.
    @handle pub struct GApplication {}
    /// `GObject` — root of the GLib reference-counted type system.
    /// Every GtkWidget / GApplication / GMenu is a GObject; cast to
    /// it for the universal `g_object_unref` /
    /// `g_signal_connect_data` calls.
    @handle pub struct GObject {}

    // ─── GApplication / GtkApplication ────────────────────────

    /// Application flags. `0` = the default (no special handling).
    /// Match `GApplicationFlags` in `gio/gioenums.h`.
    pub const G_APPLICATION_DEFAULT_FLAGS: u32 = 0 as u32

    /// Construct a new `GtkApplication`. Pass the reverse-DNS
    /// application id; the app refuses to start if the id is
    /// missing or malformed. `flags` is typically
    /// `G_APPLICATION_DEFAULT_FLAGS`.
    @lib pub fn gtk_application_new(
        application_id: *const char, flags: u32
    ): GtkApplication

    /// Drive the GTK main loop. Blocks until the application
    /// quits. `argc` / `argv` may be `0` / `0 as *const *const char`
    /// when no command-line forwarding is needed.
    @lib pub fn g_application_run(
        app: GApplication, argc: i32, argv: *const *const char
    ): i32

    /// Schedule the application to exit its `g_application_run`
    /// loop. Returns immediately; the loop unwinds on the next
    /// iteration.
    @lib pub fn g_application_quit(app: GApplication)

    // ─── GtkWindow / GtkApplicationWindow ─────────────────────

    /// Allocate a top-level window owned by `app`. The window
    /// stays hidden until `gtk_window_present` is called.
    @lib pub fn gtk_application_window_new(app: GtkApplication): GtkWindow

    @lib pub fn gtk_window_set_title(w: GtkWindow, title: *const char)
    @lib pub fn gtk_window_set_default_size(
        w: GtkWindow, width: i32, height: i32
    )

    /// Show the window. First call brings it on screen; subsequent
    /// calls re-focus.
    @lib pub fn gtk_window_present(w: GtkWindow)

    /// Hide the window without destroying it. A later
    /// `gtk_window_present` re-shows the same instance.
    @lib pub fn gtk_widget_set_visible(w: GtkWidget, visible: i32)

    /// Tear the window down. Releases the GtkApplication's hold;
    /// when the last window is destroyed the app's
    /// `g_application_run` loop exits.
    @lib pub fn gtk_window_destroy(w: GtkWindow)

    /// Install `child` as the window's content. GTK 4 uses a
    /// single-child container model — to host multiple widgets,
    /// wrap them in a `GtkFixed` (or `GtkBox` / `GtkGrid`) first
    /// and pass the container here.
    @lib pub fn gtk_window_set_child(w: GtkWindow, child: GtkWidget)

    // ─── GtkWidget basics ────────────────────────────────────

    /// Force a non-default size on `w`. libs/gui hands each widget
    /// a fixed (width, height) up front, so we drive this rather
    /// than relying on GTK's allocation negotiation.
    @lib pub fn gtk_widget_set_size_request(
        w: GtkWidget, width: i32, height: i32
    )

    /// Toggle the widget's enabled flag. Disabled widgets render
    /// dimmed and stop receiving input — same behaviour as
    /// `EnableWindow` on win32 / `setEnabled:` on cocoa.
    @lib pub fn gtk_widget_set_sensitive(w: GtkWidget, sensitive: i32)

    /// Read-side counterpart of `set_visible`.
    @lib pub fn gtk_widget_get_visible(w: GtkWidget): i32

    /// Allocated width / height of the widget after its most
    /// recent layout pass. Used for responding to window resizes.
    @lib pub fn gtk_widget_get_width(w: GtkWidget): i32
    @lib pub fn gtk_widget_get_height(w: GtkWidget): i32

    // ─── GObject / signals ───────────────────────────────────

    /// Decrement `obj`'s refcount; deallocates when it hits zero.
    /// libs/gui rarely calls this directly — widgets are owned by
    /// their parent container and freed transitively on
    /// `gtk_window_destroy`.
    @lib pub fn g_object_unref(obj: GObject)

    /// Connect a callback to a signal. `data` is forwarded as the
    /// trailing `gpointer user_data`, `destroy_data` can be `0`,
    /// `flags` is normally `0`. Returns the handler id.
    ///
    /// Two overloads cover the callback shapes libs/gui uses:
    /// `void(*)(instance, user_data)` for one-arg signals like
    /// `clicked` / `toggled` / `changed` / `value-changed` /
    /// `destroy`, and `void(*)(instance, arg, user_data)` for
    /// two-arg ones like `row-selected` and `activate`.
    @lib pub fn g_signal_connect_data(
        instance: GObject,
        signal_name: *const char,
        c_handler: fn(i64, i64),
        data: *void,
        destroy_data: i64,
        flags: u32
    ): u64
    /// Same C symbol, two-arg-payload variant. ilang's overload
    /// resolver doesn't pick the right `fn(...)` shape on its own,
    /// so this binding is a distinct ilang name that still routes
    /// to `g_signal_connect_data` at the C level via `@symbol`.
    @symbol("g_signal_connect_data")
    @lib pub fn g_signal_connect_data2(
        instance: GObject,
        signal_name: *const char,
        c_handler: fn(i64, i64, i64),
        data: *void,
        destroy_data: i64,
        flags: u32
    ): u64
}
