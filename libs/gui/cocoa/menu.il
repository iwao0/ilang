/// macOS menu backend. NSMenu / NSMenuItem wrappers behind the
/// gui-core `Menu` class. Each ilang Menu is a single NSMenu;
/// `gui.Window.setMenuBar([...])` wraps them in a top-level NSMenu
/// installed as the app's main menu via `NSApplication.setMainMenu`.

use cocoa {
    NSObject, NSApplication, NSMenu, NSMenuItem,
    nsstr, selectorPtr, sharedApplication,
}

/// Per-ilang-Menu state. Stored in `MENUS` keyed by the NSMenu's
/// raw ObjC pointer.
class MenuSlot {
    pub menu:  NSMenu
    pub title: string
    pub init(menu: NSMenu, title: string) {
        this.menu = menu
        this.title = title
    }
}

let MENUS = new Map<i64, MenuSlot>()

/// Per-NSMenuItem callback table. The shared `MenuActionHandler`
/// looks up the entry by `sender.handle` and invokes it.
let ITEM_ACTIONS = new Map<i64, fn()>()

/// Shared target. The same instance is reused as the `target` of
/// every menu item so AppKit's `setAction:` dispatch resolves
/// through one ObjC class. Allocated lazily on the first menu
/// item add.
class MenuActionHandler : NSObject {
    pub menuFired(sender: NSObject) {
        if let some(cb) = ITEM_ACTIONS.get(sender.handle) {
            cb()
        }
    }
}

let HANDLER_HOLDER: NSObject[] = []   // 0- or 1-element pseudo-Optional

fn handler(): NSObject {
    if HANDLER_HOLDER.length == 0 {
        MenuActionHandler.register()
        let h = MenuActionHandler.alloc().init()
        HANDLER_HOLDER.push(h as NSObject)
    }
    HANDLER_HOLDER[0]
}

pub fn createNativeMenu(title: string): i64 {
    let m = NSMenu.alloc().initWithTitle(nsstr(title))
    let h = m.handle
    MENUS[h] = new MenuSlot(m, title)
    h
}

pub fn menuAddItem(menuHandle: i64, text: string, onClick: fn()) {
    menuAddItemWithShortcut(menuHandle, text, "", onClick)
}

pub fn menuAddItemWithShortcut(
    menuHandle: i64, text: string, shortcut: string, onClick: fn()
) {
    if let some(slot) = MENUS.get(menuHandle) {
        let item = slot.menu.addItemWithTitle(
            nsstr(text), selectorPtr("menuFired:"), nsstr(shortcut.toLower())
        )
        item.setTarget(handler())
        ITEM_ACTIONS[item.handle] = onClick
    }
}

pub fn menuAddSeparator(menuHandle: i64) {
    if let some(slot) = MENUS.get(menuHandle) {
        slot.menu.addItem(NSMenuItem.separatorItem())
    }
}

pub fn menuAddSubmenu(parentHandle: i64, childHandle: i64) {
    if let some(parent) = MENUS.get(parentHandle) {
        if let some(child) = MENUS.get(childHandle) {
            let item = NSMenuItem.alloc().init(
                nsstr(child.title), 0 as i64, nsstr("")
            )
            item.setSubmenu(child.menu)
            parent.menu.addItem(item)
        }
    }
}

/// Install `menuHandles` as the app-wide menu bar. macOS shows
/// the main menu in the screen menu bar, not per-window — the
/// `windowHandle` is accepted for API symmetry with win32 but
/// ignored here.
pub fn setWindowMenuBar(windowHandle: i64, menuHandles: i64[]) {
    let bar = NSMenu.alloc().init()
    let n = menuHandles.length
    let i = 0
    while i < n {
        if let some(slot) = MENUS.get(menuHandles[i]) {
            let item = NSMenuItem.alloc().init(
                nsstr(slot.title), 0 as i64, nsstr("")
            )
            item.setSubmenu(slot.menu)
            bar.addItem(item)
        }
        i = i + 1
    }
    sharedApplication().setMainMenu(bar as NSObject)
}
