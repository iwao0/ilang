/// Menus — NSMenu / NSMenuItem + NSMenuDelegate.

use foundation { NSObject, NSString, NSArray, NSPoint }
use core { NSView }

@extern(ObjC, "/System/Library/Frameworks/AppKit.framework/AppKit") {
    /// `NSMenuItem` — one row in a menu. The standard ctor takes
    /// `(title, action, keyEquivalent)`; pass `selectorPtr("...")`
    /// for the action SEL (or `0` for none), and an empty string
    /// for no shortcut. Use `setSubmenu:` to nest a submenu.
    @objc pub class NSMenuItem : NSObject {
        @objc("alloc")
        pub static alloc(): NSMenuItem
        @objc("initWithTitle:action:keyEquivalent:")
        pub init(title: NSString, action: i64, keyEquivalent: NSString): NSMenuItem

        @objc("separatorItem")
        pub static separatorItem(): NSMenuItem

        @objc("setTitle:")
        pub setTitle(s: NSString)
        @objc("setTarget:")
        pub setTarget(t: NSObject)
        @objc("setAction:")
        pub setAction(sel: i64)
        @objc("setEnabled:")
        pub setEnabled(flag: bool)
        @objc("setState:")
        pub setState(s: i64)
        @objc("setSubmenu:")
        pub setSubmenu(m: NSMenu)
        @objc("setKeyEquivalent:")
        pub setKeyEquivalent(s: NSString)
    }

    /// `NSMenu` — container of `NSMenuItem`s. For the app menu
    /// bar: build a top-level NSMenu, add NSMenuItems each
    /// carrying a submenu, then `app.setMainMenu(menu)`.
    @objc pub class NSMenu : NSObject {
        @objc("alloc")
        pub static alloc(): NSMenu
        @objc("init")
        pub init(): NSMenu
        @objc("initWithTitle:")
        pub initWithTitle(title: NSString): NSMenu

        @objc("setTitle:")
        pub setTitle(s: NSString)
        @objc("addItem:")
        pub addItem(item: NSMenuItem)
        @objc("addItemWithTitle:action:keyEquivalent:")
        pub addItemWithTitle(
            title: NSString, action: i64, keyEquivalent: NSString
        ): NSMenuItem
        @objc("itemArray")
        pub get itemArray(): NSArray

        /// setDelegate overloads — typed `NSMenuDelegate` plus
        /// NSObject fallback. Same ObjC selector.
        @objc("setDelegate:")
        pub setDelegate(d: NSMenuDelegate)
        @objc("setDelegate:")
        pub setDelegate(d: NSObject)

        /// Show this menu as a popup at `location` in `view`'s
        /// coordinate space. `item` is the row that should land
        /// under the cursor (nil = position by location alone).
        @objc("popUpMenuPositioningItem:atLocation:inView:")
        pub popUpMenuPositioningItem(
            item: NSMenuItem, location: NSPoint, view: NSView
        ): bool
    }

    /// `NSMenuDelegate` — menu open / close / item validation
    /// callbacks. Attach via `menu.setDelegate(d)`; commonly used
    /// to populate menus lazily right before they're shown.
    @objc pub interface NSMenuDelegate {
        menuWillOpen?(menu: NSMenu)
        menuDidClose?(menu: NSMenu)
        menuNeedsUpdate?(menu: NSMenu)
        @objc("numberOfItemsInMenu:")
        numberOfItemsInMenu?(menu: NSMenu): i64
        @objc("menu:updateItem:atIndex:shouldCancel:")
        updateMenuItem?(menu: NSMenu, updateItem: NSMenuItem, atIndex: i64, shouldCancel: bool): bool
        @objc("menu:willHighlightItem:")
        menuWillHighlightItem?(menu: NSMenu, willHighlightItem: NSMenuItem)
    }
}
