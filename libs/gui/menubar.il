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
    menuAddSeparator,
    menuAddSubmenu,
}

pub class Menu {
    pub handle: i64

    pub init(title: string) {
        this.handle = createNativeMenu(title)
    }

    /// Append a plain command item — fires `onClick` when selected.
    pub addItem(text: string, onClick: fn()) {
        menuAddItem(this.handle, text, onClick)
    }

    /// Same as `addItem(text, onClick)` but binds a keyboard
    /// accelerator. `shortcut` is the key the user presses with the
    /// platform's command modifier — e.g. `"S"` becomes `Cmd+S` on
    /// macOS and `Ctrl+S` on Windows.
    pub addItem(text: string, shortcut: string, onClick: fn()) {
        menuAddItemWithShortcut(this.handle, text, shortcut, onClick)
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
