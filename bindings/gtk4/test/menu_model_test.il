/// gtk — exercises a display-free slice of the binding by building
/// an in-memory GMenu and dropping it. `g_menu_new` /
/// `g_menu_append` / `g_simple_action_new` are pure GLib/GObject
/// model operations and don't open the display, so the fixture
/// runs on any host that has libgtk-4 installed.

use std.test
use gtk {
    GMenu, GMenuModel, GObject, GVariantType,
    GSimpleAction, GSimpleActionGroup,
    g_menu_new, g_menu_append, g_menu_append_submenu,
    g_simple_action_new, g_simple_action_group_new,
    g_object_ref, g_object_unref,
}

@extern(C) {
    pub fn run() {
        let bar = g_menu_new()
        test.expectTrue(bar as i64 != 0)

        let file = g_menu_new()
        g_menu_append(file, cstrFromString("Quit"), cstrFromString("app.quit"))
        g_menu_append_submenu(
            bar, cstrFromString("File"), file as GMenuModel
        )

        let group = g_simple_action_group_new()
        test.expectTrue(group as i64 != 0)
        let action = g_simple_action_new(
            cstrFromString("quit"), 0 as GVariantType
        )
        test.expectTrue(action as i64 != 0)

        // Round-trip a refcount bump to verify the binding's
        // return type (GObject) flows back as a usable handle.
        let bumped = g_object_ref(bar as GObject)
        test.expect(bumped as i64, bar as i64)
        g_object_unref(bar as GObject)
        g_object_unref(bar as GObject)
        g_object_unref(file as GObject)
        g_object_unref(group as GObject)
        g_object_unref(action as GObject)
    }
}

run()
