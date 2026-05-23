/// GMenu + GAction wrapper. Each ilang Menu is a GMenu; each
/// addItem registers a fresh GSimpleAction on the singleton
/// GtkApplication so the menu's `app.menu_<n>` reference can
/// dispatch back to the user's `fn()`.
///
/// Limitations of this first pass:
/// - Separator items are accepted but render as empty space rather
///   than a divider line (`g_menu_append_section` semantics in GTK 4
///   require grouping consecutive items into one section).
/// - Keyboard accelerators are accepted on `addItemWithShortcut`
///   but not yet wired through `gtk_application_set_accels_for_action`.

use gtk {
    GMenu, GMenuModel, GAction, GSimpleAction, GActionMap,
    GtkApplication, GVariantType, GObject,
    g_menu_new, g_menu_append, g_menu_append_submenu,
    g_simple_action_new, g_action_map_add_action,
    gtk_application_set_menubar,
    g_signal_connect_data2,
}
use core { APP_HOLDER }

class MenuSlot {
    pub menu:  GMenu
    pub title: string
    pub init(menu: GMenu, title: string) {
        this.menu = menu
        this.title = title
    }
}

let MENUS = new Map<i64, MenuSlot>()
let ITEM_ACTIONS = new Map<i64, fn()>()
let NEXT_ACTION_ID: i64[] = [1 as i64]

@extern(C) {
    /// `void on_action(GSimpleAction*, GVariant*, gpointer user_data)`.
    /// `user_data` carries the action id as an integer-shaped
    /// pointer.
    pub fn linux_action_activated(action: i64, param: i64, data: i64) {
        let id = data as i64
        if let some(cb) = ITEM_ACTIONS.get(id) { cb() }
    }
}

@extern(C) {

pub fn createNativeMenu(title: string): i64 {
    let m = g_menu_new()
    let handle = m as i64
    MENUS[handle] = new MenuSlot(m, title)
    handle
}

pub fn menuAddItem(menuHandle: i64, text: string, onClick: fn()) {
    menuAddItemWithShortcut(menuHandle, text, "", onClick)
}

pub fn menuAddItemWithShortcut(
    menuHandle: i64, text: string, shortcut: string, onClick: fn()
) {
    let _ = shortcut    // wiring through set_accels_for_action is a follow-up
    if let some(slot) = MENUS.get(menuHandle) {
        let id = NEXT_ACTION_ID[0]
        NEXT_ACTION_ID[0] = id + 1 as i64
        ITEM_ACTIONS[id] = onClick

        let actionName = "menu_" + id.toString()
        let detailed   = "app." + actionName

        if APP_HOLDER.length > 0 {
            let app = APP_HOLDER[0]
            let simple = g_simple_action_new(
                cstrFromString(actionName), 0 as GVariantType
            )
            let _ = g_signal_connect_data2(
                simple as GObject,
                cstrFromString("activate"),
                linux_action_activated,
                id as *void,
                0 as i64, 0 as u32
            )
            g_action_map_add_action(app as GActionMap, simple as GAction)
        }

        g_menu_append(slot.menu, cstrFromString(text), cstrFromString(detailed))
    }
}

pub fn menuAddSeparator(menuHandle: i64) {
    let _ = menuHandle
}

pub fn menuAddSubmenu(parentHandle: i64, childHandle: i64) {
    if let some(parent) = MENUS.get(parentHandle) {
        if let some(child) = MENUS.get(childHandle) {
            g_menu_append_submenu(
                parent.menu, cstrFromString(child.title),
                child.menu as GMenuModel
            )
        }
    }
}

pub fn setWindowMenuBar(windowHandle: i64, menuHandles: i64[]) {
    let _ = windowHandle
    if APP_HOLDER.length == 0 { return }
    let bar = g_menu_new()
    let n = menuHandles.length
    let i = 0
    while i < n {
        if let some(slot) = MENUS.get(menuHandles[i]) {
            g_menu_append_submenu(
                bar, cstrFromString(slot.title), slot.menu as GMenuModel
            )
        }
        i = i + 1
    }
    let app = APP_HOLDER[0]
    gtk_application_set_menubar(app as *void, bar as GMenuModel)
}

} // @extern(C)
