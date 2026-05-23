/// GMenu + GAction wrapper. Each ilang Menu is a GMenu whose
/// items live in a "current section" sub-GMenu; addSeparator
/// finalises that section and starts a new one so GTK renders a
/// divider between groups (GTK 4 has no `g_menu_append_separator`
/// — section boundaries are the separator).
///
/// Actions go on a process-wide `GSimpleActionGroup` (`ACTION_GROUP`)
/// that `setWindowMenuBar` inserts on each window under the `"app"`
/// prefix, so menu entries can name `"app.menu_<n>"` regardless of
/// which window the menubar is attached to. This sidesteps the
/// `GtkApplication` requirement of `gtk_application_set_menubar`.
///
/// Keyboard accelerators are forwarded to a `GtkShortcutController`
/// attached to each window. The user-facing API accepts a bare
/// letter like `"N"`; we expand it to `"<Primary>n"` so the same
/// accelerator means Cmd+N on macOS-hosted GTK and Ctrl+N on Linux.

use gtk {
    GMenu, GMenuModel, GAction, GSimpleAction, GActionMap, GActionGroup,
    GSimpleActionGroup, GVariantType, GObject, GtkWidget, GtkBox,
    GtkPopoverMenuBar, GtkWindow, GtkFixed, GtkOrientation,
    GtkShortcutController, GtkShortcut, GtkShortcutTrigger,
    GtkShortcutAction, GtkShortcutScope, GtkEventController,
    g_menu_new, g_menu_append, g_menu_append_submenu, g_menu_append_section,
    g_simple_action_new, g_simple_action_group_new, g_action_map_add_action,
    gtk_popover_menu_bar_new_from_model,
    gtk_widget_insert_action_group, gtk_widget_add_controller,
    gtk_widget_set_vexpand, gtk_widget_set_hexpand,
    gtk_window_set_child,
    gtk_box_new, gtk_box_append,
    gtk_shortcut_controller_new, gtk_shortcut_controller_set_scope,
    gtk_shortcut_controller_add_shortcut,
    gtk_shortcut_new, gtk_shortcut_trigger_parse_string, gtk_named_action_new,
    g_object_ref, g_object_unref,
    g_signal_connect_data2,
}
use core { WINDOWS }

class MenuSlot {
    pub menu:    GMenu     // top-level GMenu shown in the menubar
    pub section: GMenu[]   // single-element mutable slot for the current section
    pub title:   string
    pub init(menu: GMenu, section: GMenu, title: string) {
        this.menu = menu
        this.section = [section]
        this.title = title
    }
}

let MENUS = new Map<i64, MenuSlot>()
let ITEM_ACTIONS = new Map<i64, fn()>()
let NEXT_ACTION_ID: i64[] = [1 as i64]
/// Single GSimpleActionGroup shared by every menubar — installed
/// on each window under the `"app"` prefix.
let ACTION_GROUP: GSimpleActionGroup[] = []

/// Pending `(detailed_action, accelerator)` pairs collected at
/// addItem time. `setWindowMenuBar` installs them on the window's
/// GtkShortcutController.
class PendingShortcut {
    pub action: string
    pub accel:  string
    pub init(action: string, accel: string) {
        this.action = action
        this.accel = accel
    }
}
let SHORTCUTS: PendingShortcut[] = []

@extern(C) {
    /// `void on_action(GSimpleAction*, GVariant*, gpointer user_data)`.
    /// `user_data` carries the action id as an integer-shaped
    /// pointer.
    pub fn linux_action_activated(action: i64, param: i64, data: i64) {
        let id = data as i64
        if let some(cb) = ITEM_ACTIONS.get(id) { cb() }
    }
}

/// Convert the libs/gui shortcut convention (bare letter like
/// `"N"`) into a GTK accelerator string. A string that already
/// includes a `<Modifier>` block is passed through unchanged so
/// callers can override the default Primary modifier when needed.
fn formatAccel(s: string): string {
    if s.length == 0 { "" }
    elif s.includes("<") { s }
    else { "<Primary>" + s.toLower() }
}

@extern(C) {

fn ensureActionGroup(): GSimpleActionGroup {
    if ACTION_GROUP.length == 0 {
        ACTION_GROUP.push(g_simple_action_group_new())
    }
    ACTION_GROUP[0]
}

pub fn createNativeMenu(title: string): i64 {
    let m = g_menu_new()
    let s = g_menu_new()
    g_menu_append_section(m, 0 as *const char, s as GMenuModel)
    let handle = m as i64
    MENUS[handle] = new MenuSlot(m, s, title)
    handle
}

pub fn menuAddItem(menuHandle: i64, text: string, onClick: fn()) {
    menuAddItemWithShortcut(menuHandle, text, "", onClick)
}

pub fn menuAddItemWithShortcut(
    menuHandle: i64, text: string, shortcut: string, onClick: fn()
) {
    if let some(slot) = MENUS.get(menuHandle) {
        let id = NEXT_ACTION_ID[0]
        NEXT_ACTION_ID[0] = id + 1 as i64
        ITEM_ACTIONS[id] = onClick

        let actionName = "menu_" + id.toString()
        let detailed   = "app." + actionName

        let group = ensureActionGroup()
        let simple = g_simple_action_new(
            cstrFromString(actionName), 0 as GVariantType
        )
        g_signal_connect_data2(
            simple as GObject,
            cstrFromString("activate"),
            linux_action_activated,
            id as *void,
            0 as i64, 0 as u32
        )
        g_action_map_add_action(group as GActionMap, simple as GAction)

        let accel = formatAccel(shortcut)
        if accel.length > 0 {
            SHORTCUTS.push(new PendingShortcut(detailed, accel))
        }

        g_menu_append(
            slot.section[0], cstrFromString(text), cstrFromString(detailed)
        )
    }
}

pub fn menuAddSeparator(menuHandle: i64) {
    if let some(slot) = MENUS.get(menuHandle) {
        let s = g_menu_new()
        g_menu_append_section(slot.menu, 0 as *const char, s as GMenuModel)
        slot.section[0] = s
    }
}

pub fn menuAddSubmenu(parentHandle: i64, childHandle: i64) {
    if let some(parent) = MENUS.get(parentHandle) {
        if let some(child) = MENUS.get(childHandle) {
            g_menu_append_submenu(
                parent.section[0], cstrFromString(child.title),
                child.menu as GMenuModel
            )
        }
    }
}

pub fn setWindowMenuBar(windowHandle: i64, menuHandles: i64[]) {
    if let some(wslot) = WINDOWS.get(windowHandle) {
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

        let group = ensureActionGroup()
        gtk_widget_insert_action_group(
            wslot.win as GtkWidget,
            cstrFromString("app"),
            group as GActionGroup
        )

        // Install pending menu accelerators on a fresh shortcut
        // controller. Scope = GLOBAL so the shortcut fires
        // regardless of which widget owns focus.
        let sc = gtk_shortcut_controller_new() as GtkShortcutController
        gtk_shortcut_controller_set_scope(sc, 2 as GtkShortcutScope)
        let sn = SHORTCUTS.length
        let si = 0
        while si < sn {
            let p = SHORTCUTS[si]
            let trigger = gtk_shortcut_trigger_parse_string(
                cstrFromString(p.accel)
            )
            let action = gtk_named_action_new(cstrFromString(p.action)) as GtkShortcutAction
            let shortcut = gtk_shortcut_new(trigger, action)
            gtk_shortcut_controller_add_shortcut(sc, shortcut)
            si = si + 1
        }
        gtk_widget_add_controller(
            wslot.win as GtkWidget, sc as GtkEventController
        )

        let menubar = gtk_popover_menu_bar_new_from_model(bar as GMenuModel) as GtkPopoverMenuBar
        let vbox = gtk_box_new(1 as GtkOrientation, 0 as i32) as GtkBox

        // Reparent fixed: keep an extra ref across detach so
        // gtk_window_set_child(NULL) doesn't destroy it.
        g_object_ref(wslot.fixed as GObject)
        gtk_window_set_child(wslot.win, 0 as GtkWidget)
        gtk_box_append(vbox, menubar as GtkWidget)
        gtk_box_append(vbox, wslot.fixed as GtkWidget)
        gtk_widget_set_vexpand(wslot.fixed as GtkWidget, 1 as i32)
        gtk_widget_set_hexpand(wslot.fixed as GtkWidget, 1 as i32)
        g_object_unref(wslot.fixed as GObject)
        gtk_window_set_child(wslot.win, vbox as GtkWidget)
    }
}

} // @extern(C)
