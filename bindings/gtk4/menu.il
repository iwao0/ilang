/// GMenu / GAction bindings — the model-driven menu API used by
/// `GtkApplication.set_menubar`.
///
/// GTK 4 separates menu *structure* (`GMenu`) from menu *handlers*
/// (`GAction`s registered on the application). `g_menu_append` /
/// `g_menu_append_submenu` build the tree; each item references
/// an action name like `"app.quit"` and the matching `GSimpleAction`
/// fires the actual callback.

use core { GApplication, GObject }

@extern(C, "gtk-4") {
    @handle pub struct GMenu {}
    /// `GMenuModel` is `GMenu`'s read-only base. `set_menubar` and
    /// the popover constructors take this view.
    @handle pub struct GMenuModel {}
    @handle pub struct GMenuItem {}

    /// `GAction` — the abstract action handle. `GSimpleAction` is
    /// the concrete implementation we instantiate.
    @handle pub struct GAction {}
    @handle pub struct GSimpleAction {}
    /// `GActionMap` interface — `GApplication` and
    /// `GtkApplicationWindow` both implement it. Used to register
    /// actions under the `app.` / `win.` namespace.
    @handle pub struct GActionMap {}

    /// `GVariantType` — parameter type for parameterised actions.
    /// libs/gui passes plain (zero-arg) actions, so we hand `0`
    /// here and the binding stays type-safe.
    @handle pub struct GVariantType {}
    @handle pub struct GVariant {}

    // ─── GMenu construction ──────────────────────────────────

    @lib pub fn g_menu_new(): GMenu

    /// Append a label/action pair. `detailed_action` is a string
    /// like `"app.quit"` or `"app.new"` — `GtkApplication` walks
    /// its action map by that name to find the handler.
    @lib pub fn g_menu_append(
        menu: GMenu,
        label: *const char,
        detailed_action: *const char
    )

    /// Append a submenu. `submenu` is rooted at a `GMenu` (cast to
    /// `GMenuModel` for the call).
    @lib pub fn g_menu_append_submenu(
        menu: GMenu,
        label: *const char,
        submenu: GMenuModel
    )

    /// Append a separator. The first arg is unused (no section
    /// label), so pass `0 as *const char`.
    @lib pub fn g_menu_append_section(
        menu: GMenu,
        label: *const char,
        section: GMenuModel
    )

    @lib pub fn g_menu_item_new(
        label: *const char,
        detailed_action: *const char
    ): GMenuItem
    @lib pub fn g_menu_item_set_accel(
        item: GMenuItem,
        accel: *const char
    )
    @lib pub fn g_menu_append_item(menu: GMenu, item: GMenuItem)

    // ─── GAction / GApplication wiring ───────────────────────

    /// Build a plain (zero-parameter) action. The activate
    /// callback fires every time the user picks a menu item bound
    /// to `name`.
    @lib pub fn g_simple_action_new(
        name: *const char,
        parameter_type: GVariantType
    ): GSimpleAction

    @lib pub fn g_action_map_add_action(
        map: GActionMap,
        action: GAction
    )

    /// Install `model` as the app-wide menubar. Each
    /// `GtkApplicationWindow` it owns renders the menubar
    /// automatically (with `show-menubar` set on the window).
    @lib pub fn gtk_application_set_menubar(
        app: *void, model: GMenuModel
    )

    /// Bind a keyboard accelerator to a detailed action. `accels`
    /// is a NULL-terminated `*const *const char` of strings like
    /// `"<Primary>S"` (Primary = Cmd on macOS, Ctrl elsewhere).
    @lib pub fn gtk_application_set_accels_for_action(
        app: *void,
        detailed_action: *const char,
        accels: *const *const char
    )
}
