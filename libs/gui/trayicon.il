/// Status-bar / notification-area icon. Lives outside any
/// `Window` — closing the last window doesn't dismiss the tray
/// icon. Construct, configure, then `show()` to install it.
///
///   let tray = new gui.TrayIcon()
///   tray.setIcon("/path/to/icon.png")
///   tray.setTooltip("My App")
///   tray.onClick.add(fn(e: gui.MouseEvent) { console.log("click") })
///   let menu = new gui.Menu("")
///   menu.addItem(new gui.MenuItem("Open", fn() { ... }))
///   menu.addItem(new gui.MenuItem("Quit", fn() { gui.quit() }))
///   tray.setMenu(menu)
///   tray.show()
///
/// `setMenu` enables right-click (Cocoa: also left-click) →
/// menu pop-up. `onClick` fires for plain left-click — fires
/// even when a menu is installed on platforms that distinguish
/// the two gestures (win32). On macOS a menu, if any, has
/// precedence over the click handler.
///
/// Linux: GTK 4 dropped its native tray support and the
/// successor (libayatana-appindicator3) isn't bound here yet,
/// so the linux backend is a no-op stub.

use std.events { Signal }
use events { EventTarget, MouseEvent }
use menubar { Menu }
use gui_impl {
    createNativeTrayIcon, removeNativeTrayIcon,
    setTrayIconIcon, setTrayIconTitle, setTrayIconTooltip,
    setTrayIconMenu,
    showTrayIcon, setOnTrayIconClick,
}

pub class TrayIcon : EventTarget {
    pub handle:     i64
    pub _iconPath:  string
    pub _title:     string
    pub _tooltip:   string
    pub _menuHandle: i64
    pub onClick:    Signal<MouseEvent>

    pub init() {
        super()
        this.handle      = -1
        this._iconPath   = ""
        this._title      = ""
        this._tooltip    = ""
        this._menuHandle = -1
        this.onClick     = new Signal<MouseEvent>()
    }

    pub setIcon(path: string) {
        this._iconPath = path
        if this.handle >= 0 {
            setTrayIconIcon(this.handle, path)
        }
    }

    /// Text fallback when no icon image is set — useful for
    /// quick demos and for textual indicators like "•" or
    /// emoji. macOS shows it in the menu-bar item directly;
    /// win32 / linux currently ignore it (the tray APIs there
    /// expect an image).
    pub setTitle(text: string) {
        this._title = text
        if this.handle >= 0 {
            setTrayIconTitle(this.handle, text)
        }
    }

    pub setTooltip(text: string) {
        this._tooltip = text
        if this.handle >= 0 {
            setTrayIconTooltip(this.handle, text)
        }
    }

    /// Bind a `Menu` as the icon's context menu. The menu's
    /// label / accelerator is ignored — only its items matter.
    pub setMenu(menu: Menu) {
        this._menuHandle = menu.handle
        if this.handle >= 0 {
            setTrayIconMenu(this.handle, menu.handle)
        }
    }

    /// Install the icon in the system status bar / notification
    /// area. Subsequent `setIcon` / `setTooltip` / `setMenu`
    /// edits apply live. Idempotent — calling `show` on an
    /// already-shown icon is a no-op.
    pub show() {
        if this.handle >= 0 {
            showTrayIcon(this.handle, true)
            return
        }
        this.handle = createNativeTrayIcon(
            this._iconPath, this._tooltip, this._menuHandle
        )
        if this.handle >= 0 && this._title != "" {
            setTrayIconTitle(this.handle, this._title)
        }
        if this.handle >= 0 {
            // Forward backend click → our Signal.
            let signal = this.onClick
            let sender = this
            setOnTrayIconClick(this.handle, fn() {
                signal.emit(new MouseEvent(
                    sender, 0.0, 0.0, 0, false, false, false, false
                ))
            })
        }
    }

    /// Hide the icon temporarily without releasing the OS slot.
    /// Use `remove` to drop the icon entirely.
    pub hide() {
        if this.handle >= 0 {
            showTrayIcon(this.handle, false)
        }
    }

    /// Detach the icon from the OS. After this the `TrayIcon`
    /// can't be reused — construct a new one to install again.
    pub remove() {
        if this.handle >= 0 {
            removeNativeTrayIcon(this.handle)
            this.handle = -1
        }
    }
}
