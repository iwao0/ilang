/// Menu — one branch of a window's menu bar (or a stand-alone
/// popup menu). Build the tree before showing:
///
///     let file = new gui.Menu("File")
///     file.addItem("New", "N", fn() { ... })
///     file.addItem("Open", "O", fn() { ... })
///     file.addSeparator()
///     file.addItem("Quit", "Q", fn() { ... })
///
///     window.setMenuBar([file, helpMenu])

use gui_impl {
    createNativeMenu,
    menuAddItem,
    menuAddItemWithShortcut,
    menuAddCheckItem,
    menuAddRadioItem,
    menuAddSeparator,
    menuAddSubmenu,
    menuItemSetEnabled,
    menuItemSetChecked,
}

/// Single row inside a `Menu`. Returned by `addItem` /
/// `addCheckItem` / `addRadioItem` so callers can flip the row's
/// enabled / checked state after it's been added.
pub class MenuItem {
    /// Native id — opaque on the public side. cocoa: NSMenuItem
    /// ObjC pointer; win32: WM_COMMAND id; linux: GAction id.
    pub handle:     i64
    pub menuHandle: i64
    _isEnabled: bool
    _isChecked: bool

    pub init(handle: i64, menuHandle: i64) {
        this.handle     = handle
        this.menuHandle = menuHandle
        this._isEnabled = true
        this._isChecked = false
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        menuItemSetEnabled(this.menuHandle, this.handle, v)
    }

    /// Toggle the check / radio mark on this item. No-op for
    /// regular `addItem` rows on platforms that ignore the state
    /// for plain command items.
    pub get isChecked(): bool { this._isChecked }
    pub set isChecked(v: bool) {
        this._isChecked = v
        menuItemSetChecked(this.menuHandle, this.handle, v)
    }
}

pub class Menu {
    pub handle: i64

    pub init(title: string) {
        this.handle = createNativeMenu(title)
    }

    /// Append a plain command item — fires `onClick` when selected.
    pub addItem(text: string, onClick: fn()): MenuItem {
        let id = menuAddItem(this.handle, text, onClick)
        new MenuItem(id, this.handle)
    }

    /// Same as `addItem(text, onClick)` but binds a keyboard
    /// accelerator. `shortcut` is the key the user presses with the
    /// platform's command modifier — e.g. `"S"` becomes `Cmd+S` on
    /// macOS and `Ctrl+S` on Windows.
    pub addItem(text: string, shortcut: string, onClick: fn()): MenuItem {
        let id = menuAddItemWithShortcut(this.handle, text, shortcut, onClick)
        new MenuItem(id, this.handle)
    }

    /// Append a check item — toggles between checked / unchecked
    /// on each selection. `onToggle` is fired with the new state
    /// after the toggle.
    pub addCheckItem(text: string, initialChecked: bool, onToggle: fn(bool)): MenuItem {
        let id = menuAddCheckItem(this.handle, text, initialChecked, onToggle)
        let item = new MenuItem(id, this.handle)
        item._isChecked = initialChecked
        item
    }

    /// Append a radio item belonging to `groupName`. Selecting it
    /// clears the check mark on every other radio item that
    /// shares the same group within this menu and fires `onSelect`.
    pub addRadioItem(text: string, groupName: string, onSelect: fn()): MenuItem {
        let id = menuAddRadioItem(this.handle, text, groupName, onSelect)
        new MenuItem(id, this.handle)
    }

    /// Append a horizontal separator line.
    pub addSeparator() {
        menuAddSeparator(this.handle)
    }

    /// Append `submenu` as a nested menu under this one. The
    /// submenu's title is the parent-side label.
    pub addSubmenu(submenu: Menu) {
        menuAddSubmenu(this.handle, submenu.handle)
    }
}
