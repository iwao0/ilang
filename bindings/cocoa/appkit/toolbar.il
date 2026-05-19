// NSToolbar / NSToolbarItem / NSTabView / NSTabViewItem /
// NSStatusBar / NSStatusItem — UI chrome around the main content.

use foundation { NSObject, NSString, NSArray, NSNotification, NSRect }
use core { NSView }
use image { NSImage }

// NSToolbarDisplayMode — what each toolbar slot shows.
pub enum NSToolbarDisplayMode: u64 {
    default       = 0
    iconAndLabel  = 1
    iconOnly      = 2
    labelOnly     = 3
}

// NSToolbarSizeMode — icon size preset for the toolbar.
pub enum NSToolbarSizeMode: u64 {
    default = 0
    regular = 1
    small   = 2
}

// NSTabViewType — which edge of the view the tab strip lives
// on, or whether tabs are hidden entirely (`noTabsBezelBorder`
// for a borderless container).
pub enum NSTabViewType: u64 {
    topTabsBezelBorder    = 0
    leftTabsBezelBorder   = 1
    bottomTabsBezelBorder = 2
    rightTabsBezelBorder  = 3
    noTabsBezelBorder     = 4
    noTabsLineBorder      = 5
    noTabsNoBorder        = 6
}

// NSStatusItemLength — sentinel values for
// `NSStatusItem.setLength:` indicating the auto-sizing
// behaviour rather than a literal pixel width.
pub const StatusItemVariableLength: f64 = -1.0
pub const StatusItemSquareLength: f64 = -2.0

@extern(ObjC, "/System/Library/Frameworks/AppKit.framework/AppKit") {
    /// `NSToolbar` — window toolbar. Set as
    /// `window.setToolbar(t)`. Items are sourced through a
    /// delegate (an `NSToolbarDelegate` impl) that maps each
    /// item-identifier string to an `NSToolbarItem`.
    @objc pub class NSToolbar : NSObject {
        @objc("alloc")
        pub static alloc(): NSToolbar
        @objc("initWithIdentifier:")
        pub initWithIdentifier(id: NSString): NSToolbar

        @objc("identifier")
        pub get identifier(): NSString
        @objc("displayMode")
        pub get displayMode(): NSToolbarDisplayMode
        @objc("setDisplayMode:")
        pub set displayMode(m: NSToolbarDisplayMode)
        @objc("sizeMode")
        pub get sizeMode(): NSToolbarSizeMode
        @objc("setSizeMode:")
        pub set sizeMode(m: NSToolbarSizeMode)
        @objc("isVisible")
        pub get isVisible(): bool
        @objc("setVisible:")
        pub set isVisible(flag: bool)
        @objc("showsBaselineSeparator")
        pub get showsBaselineSeparator(): bool
        @objc("setShowsBaselineSeparator:")
        pub set showsBaselineSeparator(flag: bool)
        @objc("allowsUserCustomization")
        pub get allowsUserCustomization(): bool
        @objc("setAllowsUserCustomization:")
        pub set allowsUserCustomization(flag: bool)
        @objc("autosavesConfiguration")
        pub get autosavesConfiguration(): bool
        @objc("setAutosavesConfiguration:")
        pub set autosavesConfiguration(flag: bool)

        @objc("items")
        pub get items(): NSArray
        @objc("visibleItems")
        pub get visibleItems(): NSArray
        @objc("insertItemWithItemIdentifier:atIndex:")
        pub insertItemWithItemIdentifier(id: NSString, atIndex: i64)
        @objc("removeItemAtIndex:")
        pub removeItemAtIndex(i: i64)
        @objc("validateVisibleItems")
        pub validateVisibleItems()
        @objc("runCustomizationPalette:")
        pub runCustomizationPalette(sender: NSObject)
        @objc("setDelegate:")
        pub setDelegate(d: NSToolbarDelegate)
        @objc("setDelegate:")
        pub setDelegate(d: NSObject)
    }

    /// `NSToolbarItem` — single item in a toolbar. Constructed
    /// per identifier inside the delegate's
    /// `toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:`.
    /// Set the label / image / target+action and the toolbar
    /// uses them.
    @objc pub class NSToolbarItem : NSObject {
        @objc("alloc")
        pub static alloc(): NSToolbarItem
        @objc("initWithItemIdentifier:")
        pub initWithItemIdentifier(id: NSString): NSToolbarItem

        @objc("itemIdentifier")
        pub get itemIdentifier(): NSString
        @objc("label")
        pub get label(): NSString
        @objc("setLabel:")
        pub set label(s: NSString)
        @objc("paletteLabel")
        pub get paletteLabel(): NSString
        @objc("setPaletteLabel:")
        pub set paletteLabel(s: NSString)
        @objc("toolTip")
        pub get toolTip(): NSString
        @objc("setToolTip:")
        pub set toolTip(s: NSString)
        @objc("image")
        pub get image(): NSImage
        @objc("setImage:")
        pub set image(i: NSImage)
        @objc("setTarget:")
        pub setTarget(t: NSObject)
        @objc("setAction:")
        pub setAction(sel: i64)
        @objc("isEnabled")
        pub get isEnabled(): bool
        @objc("setEnabled:")
        pub set isEnabled(flag: bool)
        @objc("view")
        pub get view(): NSView
        @objc("setView:")
        pub set view(v: NSView)
    }

    /// `NSToolbarDelegate` — wires identifiers to NSToolbarItem
    /// instances and decides which identifiers are allowed /
    /// default. `toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:`
    /// is required.
    @objc pub interface NSToolbarDelegate {
        @objc("toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:")
        toolbarItemForItemIdentifier(
            toolbar: NSToolbar, itemForItemIdentifier: NSString,
            willBeInsertedIntoToolbar: bool
        ): NSToolbarItem
        @objc("toolbarDefaultItemIdentifiers:")
        toolbarDefaultItemIdentifiers(toolbar: NSToolbar): NSArray
        @objc("toolbarAllowedItemIdentifiers:")
        toolbarAllowedItemIdentifiers(toolbar: NSToolbar): NSArray

        @optional toolbarWillAddItem(notification: NSNotification)
        @optional toolbarDidRemoveItem(notification: NSNotification)
        @optional @objc("toolbarSelectableItemIdentifiers:")
        toolbarSelectableItemIdentifiers(toolbar: NSToolbar): NSArray
    }

    /// `NSTabView` — switchable container with one tab per
    /// content view. Add items via `addTabViewItem:`; the
    /// selection follows whichever item is `selectTabViewItem:`
    /// 'd last.
    @objc pub class NSTabView : NSView {
        @objc("alloc")
        pub static alloc(): NSTabView
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSTabView

        @objc("tabViewItems")
        pub get tabViewItems(): NSArray
        @objc("numberOfTabViewItems")
        pub get numberOfTabViewItems(): i64
        @objc("addTabViewItem:")
        pub addTabViewItem(item: NSTabViewItem)
        @objc("insertTabViewItem:atIndex:")
        pub insertTabViewItem(item: NSTabViewItem, atIndex: i64)
        @objc("removeTabViewItem:")
        pub removeTabViewItem(item: NSTabViewItem)
        @objc("indexOfTabViewItem:")
        pub indexOfTabViewItem(item: NSTabViewItem): i64
        @objc("tabViewItemAtIndex:")
        pub tabViewItemAtIndex(i: i64): NSTabViewItem
        @objc("selectedTabViewItem")
        pub get selectedTabViewItem(): NSTabViewItem
        @objc("selectTabViewItem:")
        pub selectTabViewItem(item: NSTabViewItem)
        @objc("selectTabViewItemAtIndex:")
        pub selectTabViewItemAtIndex(i: i64)
        @objc("selectFirstTabViewItem:")
        pub selectFirstTabViewItem(sender: NSObject)
        @objc("selectLastTabViewItem:")
        pub selectLastTabViewItem(sender: NSObject)
        @objc("selectNextTabViewItem:")
        pub selectNextTabViewItem(sender: NSObject)
        @objc("selectPreviousTabViewItem:")
        pub selectPreviousTabViewItem(sender: NSObject)

        @objc("tabViewType")
        pub get tabViewType(): NSTabViewType
        @objc("setTabViewType:")
        pub set tabViewType(t: NSTabViewType)

        @objc("setDelegate:")
        pub setDelegate(d: NSTabViewDelegate)
        @objc("setDelegate:")
        pub setDelegate(d: NSObject)
    }

    /// `NSTabViewItem` — single tab in an NSTabView. Has a
    /// label, a content view, and an identifier the tab view
    /// can look it up by.
    @objc pub class NSTabViewItem : NSObject {
        @objc("alloc")
        pub static alloc(): NSTabViewItem
        @objc("initWithIdentifier:")
        pub initWithIdentifier(id: NSObject): NSTabViewItem

        @objc("identifier")
        pub get identifier(): NSObject
        @objc("setIdentifier:")
        pub set identifier(id: NSObject)
        @objc("label")
        pub get label(): NSString
        @objc("setLabel:")
        pub set label(s: NSString)
        @objc("toolTip")
        pub get toolTip(): NSString
        @objc("setToolTip:")
        pub set toolTip(s: NSString)
        @objc("view")
        pub get view(): NSView
        @objc("setView:")
        pub set view(v: NSView)
        @objc("color")
        pub get color(): NSObject
        @objc("setColor:")
        pub set color(c: NSObject)
    }

    /// `NSTabViewDelegate` — tab selection callbacks.
    @objc pub interface NSTabViewDelegate {
        @optional @objc("tabView:shouldSelectTabViewItem:")
        tabViewShouldSelectTabViewItem(
            tabView: NSTabView, shouldSelectTabViewItem: NSTabViewItem
        ): bool
        @optional @objc("tabView:willSelectTabViewItem:")
        tabViewWillSelectTabViewItem(
            tabView: NSTabView, willSelectTabViewItem: NSTabViewItem
        )
        @optional @objc("tabView:didSelectTabViewItem:")
        tabViewDidSelectTabViewItem(
            tabView: NSTabView, didSelectTabViewItem: NSTabViewItem
        )
        @optional tabViewDidChangeNumberOfTabViewItems(tabView: NSTabView)
    }

    /// `NSStatusBar` — process-wide menu-bar status area
    /// (top-right system menu bar). `systemStatusBar()` is the
    /// shared instance; create per-app status items via
    /// `statusItemWithLength:`.
    @objc pub class NSStatusBar : NSObject {
        @objc("systemStatusBar")
        pub static systemStatusBar(): NSStatusBar

        @objc("statusItemWithLength:")
        pub statusItemWithLength(length: f64): NSStatusItem
        @objc("removeStatusItem:")
        pub removeStatusItem(item: NSStatusItem)
        @objc("isVertical")
        pub get isVertical(): bool
        @objc("thickness")
        pub get thickness(): f64
    }

    /// `NSStatusItem` — single tray entry in the system menu
    /// bar. Pass `cocoa.StatusItemVariableLength` /
    /// `cocoa.StatusItemSquareLength` as the length to opt into
    /// the auto-sizing behaviours.
    @objc pub class NSStatusItem : NSObject {
        @objc("button")
        pub get button(): NSObject
        @objc("menu")
        pub get menu(): NSObject
        @objc("setMenu:")
        pub set menu(m: NSObject)
        @objc("length")
        pub get length(): f64
        @objc("setLength:")
        pub set length(l: f64)
        @objc("isVisible")
        pub get isVisible(): bool
        @objc("setVisible:")
        pub set isVisible(flag: bool)
        @objc("behavior")
        pub get behavior(): u64
        @objc("setBehavior:")
        pub set behavior(b: u64)
    }
}
