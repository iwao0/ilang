// AppKit bindings — the macOS GUI layer (NSApplication / NSWindow
// / NSView / NSButton / NSTextField / NSSlider / NSStackView)
// plus the typed enums and `make*` convenience helpers that go
// with them. Depends on `foundation` for NSObject / NSString /
// geometry types.

use foundation { NSObject, NSString, NSArray, NSDate, NSURL, NSNotification, NSPoint, NSSize, NSRect, nsstr, ilstr, makeRect }

// ─── NSWindow.StyleMask ──────────────────────────────────────────
// Mirrors Swift's `NSWindow.StyleMask` (raw bit values from
// `<AppKit/NSWindow.h>` `NSWindowStyleMask*`). `borderless` is
// the empty mask; combine the rest with `|`:
//
//     let mask = cocoa.StyleMask.titled
//         | cocoa.StyleMask.closable
//         | cocoa.StyleMask.miniaturizable
//         | cocoa.StyleMask.resizable
@flags
pub enum StyleMask: u64 {
    borderless             = 0x0000
    titled                 = 0x0001
    closable               = 0x0002
    miniaturizable         = 0x0004
    resizable              = 0x0008
    utilityWindow          = 0x0010
    docModalWindow         = 0x0040
    nonactivatingPanel     = 0x0080
    texturedBackground     = 0x0100
    unifiedTitleAndToolbar = 0x1000
    hudWindow              = 0x2000
    fullScreen             = 0x4000
    fullSizeContentView    = 0x8000
}

// NSBackingStoreType — `buffered` is the only non-deprecated
// member today; AppKit still accepts the legacy values for
// source-compat.
pub enum BackingStore: u64 {
    retained    = 0
    nonretained = 1
    buffered    = 2
}

// NSApplicationActivationPolicy — controls the app's Dock /
// activation behaviour.
pub enum ActivationPolicy: i64 {
    regular    = 0
    accessory  = 1
    prohibited = 2
}

// NSButton.BezelStyle — visual style of an NSButton. `push` is
// the standard rounded Mac push button (was named `rounded` in
// older AppKit headers).
pub enum BezelStyle: u64 {
    automatic          = 0
    push               = 1
    flexiblePush       = 2
    disclosure         = 5
    circular           = 7
    helpButton         = 9
    smallSquare        = 10
    toolbar            = 11
    accessoryBarAction = 12
    accessoryBar       = 13
    pushDisclosure     = 14
    badge              = 15
}

// NSUserInterfaceLayoutOrientation — `NSStackView.setOrientation`
// switches its arranged children between row and column layouts.
pub enum LayoutOrientation: i64 {
    horizontal = 0
    vertical   = 1
}

// NSAlertStyle — visual severity of an `NSAlert`. Affects the
// icon and accent the system draws.
pub enum AlertStyle: i64 {
    warning       = 0
    informational = 1
    critical      = 2
}

// NSModalResponse — return value of `runModal` on save / open
// panels and (for the OK/Cancel pair) modal sheets. NSAlert uses
// a separate range starting at 1000 for "first button", "second
// button", "third button" in addition order.
pub const ModalResponseOK: i64 = 1
pub const ModalResponseCancel: i64 = 0
pub const AlertFirstButtonReturn: i64 = 1000
pub const AlertSecondButtonReturn: i64 = 1001
pub const AlertThirdButtonReturn: i64 = 1002

// NSBorderType — frame style for `NSScrollView` / `NSBox`.
pub enum BorderType: u64 {
    noBorder     = 0
    lineBorder   = 1
    bezelBorder  = 2
    grooveBorder = 3
}

// NSBoxType — visual role of an `NSBox`. `primary` is the
// titled-frame default; `separator` draws as a horizontal /
// vertical rule.
pub enum BoxType: u64 {
    primary   = 0
    secondary = 1
    separator = 2
    oldStyle  = 3
    custom    = 4
}

// NSProgressIndicatorStyle — bar (determinate / linear) vs.
// spinning (indeterminate circular).
pub enum ProgressIndicatorStyle: u64 {
    bar      = 0
    spinning = 1
}

// NSSegmentSwitchTracking — how an `NSSegmentedControl` reacts
// to clicks: single-select, multi-select, or momentary
// highlight (no persistent selection).
pub enum SegmentSwitchTracking: u64 {
    selectOne = 0
    selectAny = 1
    momentary = 2
}

// NSDatePickerStyle — visual form of an `NSDatePicker`.
pub enum DatePickerStyle: u64 {
    textFieldAndStepper = 0
    clockAndCalendar    = 1
    textField           = 2
}

// Standard `NSPasteboard` type for UTF-8 plain text.
pub const PasteboardTypeString: string = "public.utf8-plain-text"

// NSEventType — the discriminator for `NSEvent.eventType()`.
// Only the values you'll see in a typical app are listed; full
// set is in `<AppKit/NSEvent.h>`.
pub enum EventType: u64 {
    leftMouseDown      = 1
    leftMouseUp        = 2
    rightMouseDown     = 3
    rightMouseUp       = 4
    mouseMoved         = 5
    leftMouseDragged   = 6
    rightMouseDragged  = 7
    mouseEntered       = 8
    mouseExited        = 9
    keyDown            = 10
    keyUp              = 11
    flagsChanged       = 12
    scrollWheel        = 22
    otherMouseDown     = 25
    otherMouseUp       = 26
    otherMouseDragged  = 27
}

// NSEventModifierFlags — bitmask read off `event.modifierFlags()`.
// Bitwise-OR the relevant flags to test composite modifiers.
@flags
pub enum EventModifierFlags: u64 {
    none      = 0
    capsLock  = 0x10000
    shift     = 0x20000
    control   = 0x40000
    option    = 0x80000
    command   = 0x100000
    numericPad = 0x200000
    help      = 0x400000
    function  = 0x800000
}

// The `@extern(ObjC, "path")` form names the framework dylib so
// the JIT dlopens it eagerly at startup — that's what registers
// `NSApplication` / `NSWindow` / NSString etc. with libobjc's
// global class registry, so the `@objc` desugar's
// `objc_getClass("NSWindow")` calls below can find them.
//
// Inside the block, items are explicitly marked:
//   * `@objc(...)` on a class / method → objc_msgSend dispatch.
//   * `@lib` (no args) on a fn → dlsym this C symbol from one
//     of the block's listed dylibs. `NSApplicationLoad` is the
//     framework's own plain C entry point.
@extern(ObjC, "/System/Library/Frameworks/AppKit.framework/AppKit") {
    @lib pub fn NSApplicationLoad(): bool

    /// The shared application object — there's only ever one per
    /// process. Use `cocoa.sharedApplication()` (defined below)
    /// instead of calling this directly so AppKit's framework
    /// load happens first.
    @objc pub class NSApplication : NSObject {
        @objc("sharedApplication")
        pub static sharedApplication(): NSApplication
        @objc("setActivationPolicy:")
        pub setActivationPolicy(policy: ActivationPolicy): bool
        @objc("activateIgnoringOtherApps:")
        @deprecated("deprecated in macOS 14.0 — call `activate()` instead.")
        pub activateIgnoringOtherApps(flag: bool)
        @objc("activate")
        pub activate()
        @objc("run")
        pub run()
        @objc("terminate:")
        pub terminate(sender: NSObject)
        // NSApplicationDelegate is the protocol an implementing
        // class should declare via `class MyApp : NSApplicationDelegate {…}`.
        // The setDelegate slot itself is typed as the most permissive
        // form (NSObject) so a one-off `setDelegate(nil)` to clear
        // it, or framework-provided delegate objects bound elsewhere
        // as bare NSObject, still go through.
        @objc("setDelegate:")
        pub setDelegate(d: NSObject)
        @objc("setMainMenu:")
        pub setMainMenu(menu: NSMenu)
    }

    /// NSView — base class for every visible AppKit object.
    @objc pub class NSView : NSObject {
        @objc("addSubview:")
        pub addSubview(v: NSView)
        // Coordinate conversion. The `fromView` / `toView` slot
        // is typed `NSObject` so `cocoa.nil` can be passed to
        // mean "convert from / to window coordinates" — the
        // same coordinate space `NSEvent.locationInWindow()`
        // returns. A real NSView wrapper is accepted too
        // (ObjC dispatches the same way either way).
        @objc("convertPoint:fromView:")
        pub convertPointFromView(p: NSPoint, fromView: NSObject): NSPoint
        @objc("convertPoint:toView:")
        pub convertPointToView(p: NSPoint, toView: NSObject): NSPoint
        @objc("frame")
        pub get frame(): NSRect
        @objc("bounds")
        pub get bounds(): NSRect
    }

    /// NSWindow. The two-step `alloc + initWithContentRect:...`
    /// pattern is the standard Cocoa instance-construction
    /// idiom; the convenience constructor below combines them.
    @objc pub class NSWindow : NSObject {
        @objc("alloc")
        pub static alloc(): NSWindow
        // `NSWindowStyleMask` / `NSBackingStoreType` are typed
        // enums on the ilang side. The MIR codegen extracts the
        // discriminant when handing them to objc_msgSend, so the
        // C ABI sees the raw NSUInteger that AppKit expects.
        @objc("initWithContentRect:styleMask:backing:defer:")
        pub init(
            rect: NSRect, mask: StyleMask, backing: BackingStore, defer: bool
        ): NSWindow

        @objc("setTitle:")
        pub setTitle(title: NSString)
        @objc("center")
        pub center()
        @objc("makeKeyAndOrderFront:")
        pub makeKeyAndOrderFront(sender: NSObject)
        @objc("contentView")
        pub get contentView(): NSView
        // NSWindowDelegate protocol — declare conformance on the
        // implementing class side (`class MyWin : NSWindowDelegate`)
        // and pass the instance here as an NSObject.
        @objc("setDelegate:")
        pub setDelegate(d: NSObject)
    }

    /// NSButton. `alloc + initWithFrame:` follows the same
    /// two-step pattern. Target/action follows the standard
    /// Cocoa convention: pair `setTarget(handler)` with
    /// `setAction(selectorPtr("clicked:"))` so AppKit invokes
    /// `[handler clicked:button]` whenever the button is clicked.
    @objc pub class NSButton : NSView {
        @objc("alloc")
        pub static alloc(): NSButton
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSButton

        @objc("setTitle:")
        pub setTitle(title: NSString)
        @objc("setBezelStyle:")
        pub setBezelStyle(style: BezelStyle)

        @objc("setTarget:")
        pub setTarget(target: NSObject)
        // SEL doesn't have a user-facing ilang type today; the
        // action is passed as the raw SEL pointer cast to `i64`.
        // Use `selectorPtr("clicked:")` to build one from a name.
        @objc("setAction:")
        pub setAction(sel: i64)
    }

    /// NSTextField. Doubles as both an editable text input and a
    /// non-editable label (set `setEditable(false)` + `setBordered(false)` +
    /// `setDrawsBackground(false)` for the label look — `makeLabel`
    /// below does that for you).
    @objc pub class NSTextField : NSView {
        @objc("alloc")
        pub static alloc(): NSTextField
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSTextField

        @objc("stringValue")
        pub get stringValue(): NSString
        @objc("setStringValue:")
        pub set stringValue(s: NSString)

        @objc("setEditable:")
        pub setEditable(flag: bool)
        @objc("setBordered:")
        pub setBordered(flag: bool)
        @objc("setBezeled:")
        pub setBezeled(flag: bool)
        @objc("setDrawsBackground:")
        pub setDrawsBackground(flag: bool)
        @objc("setSelectable:")
        pub setSelectable(flag: bool)

        // NSTextFieldDelegate / NSControlTextEditingDelegate
        // protocols cover the edit callbacks; declare conformance
        // on the implementing class and pass the instance through
        // here as an NSObject.
        @objc("setDelegate:")
        pub setDelegate(d: NSObject)
    }

    /// NSSlider — continuous horizontal slider by default. `min`,
    /// `max`, and the current `doubleValue` are f64; the value
    /// reads back via `doubleValue()` (target/action hookups for
    /// live updates follow the NSButton pattern).
    @objc pub class NSSlider : NSView {
        @objc("alloc")
        pub static alloc(): NSSlider
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSSlider

        @objc("setMinValue:")
        pub setMinValue(v: f64)
        @objc("setMaxValue:")
        pub setMaxValue(v: f64)
        @objc("doubleValue")
        pub get doubleValue(): f64
        @objc("setDoubleValue:")
        pub set doubleValue(v: f64)

        @objc("setTarget:")
        pub setTarget(target: NSObject)
        @objc("setAction:")
        pub setAction(sel: i64)
    }

    /// NSStackView — stacks its arranged subviews along
    /// `orientation` (use `cocoa.LayoutOrientation.horizontal` /
    /// `cocoa.LayoutOrientation.vertical`).
    /// `addArrangedSubview:` appends a view; the stack handles
    /// sizing and positioning.
    @objc pub class NSStackView : NSView {
        @objc("alloc")
        pub static alloc(): NSStackView
        @objc("init")
        pub init(): NSStackView

        @objc("setOrientation:")
        pub setOrientation(o: LayoutOrientation)
        @objc("setSpacing:")
        pub setSpacing(s: f64)
        @objc("addArrangedSubview:")
        pub addArrangedSubview(v: NSView)
        @objc("setFrame:")
        pub setFrame(rect: NSRect)
    }

    /// `NSColor` — a colour value. Use the named system-colour
    /// static getters (`NSColor.black`, `NSColor.red`,
    /// `NSColor.clear`, …) or `NSColor.rgba(r, g, b, a)` for RGBA
    /// values in `[0, 1]`. RGBA component accessors only make sense
    /// on a colour that lives in an RGB colour space.
    @objc pub class NSColor : NSObject {
        @objc("blackColor")
        pub static get black(): NSColor
        @objc("whiteColor")
        pub static get white(): NSColor
        @objc("clearColor")
        pub static get clear(): NSColor
        @objc("redColor")
        pub static get red(): NSColor
        @objc("greenColor")
        pub static get green(): NSColor
        @objc("blueColor")
        pub static get blue(): NSColor
        @objc("grayColor")
        pub static get gray(): NSColor
        @objc("yellowColor")
        pub static get yellow(): NSColor
        @objc("orangeColor")
        pub static get orange(): NSColor
        @objc("purpleColor")
        pub static get purple(): NSColor

        @objc("colorWithRed:green:blue:alpha:")
        pub static rgba(r: f64, g: f64, b: f64, a: f64): NSColor

        @objc("redComponent")
        pub get redComponent(): f64
        @objc("greenComponent")
        pub get greenComponent(): f64
        @objc("blueComponent")
        pub get blueComponent(): f64
        @objc("alphaComponent")
        pub get alphaComponent(): f64
    }

    /// `NSImage` — bitmap / vector image. Construct from a
    /// filesystem path (`fromFile`), an asset-catalog / named
    /// system image (`named` — returns nil if not found), or by
    /// pointing at an `NSURL`.
    @objc pub class NSImage : NSObject {
        @objc("alloc")
        pub static alloc(): NSImage
        @objc("initWithContentsOfFile:")
        pub initWithContentsOfFile(path: NSString): NSImage
        @objc("initWithContentsOfURL:")
        pub initWithContentsOfURL(url: NSURL): NSImage
        @objc("imageNamed:")
        pub static imageNamed(name: NSString): NSImage

        @objc("size")
        pub get size(): NSSize
        @objc("setSize:")
        pub set size(s: NSSize)
        @objc("isValid")
        pub get isValid(): bool
    }

    /// `NSImageView` — view that draws an `NSImage`. Build with
    /// `imageViewWithImage:` (auto-sizes to the image) or the
    /// usual `alloc + initWithFrame:` for an explicit frame.
    @objc pub class NSImageView : NSView {
        @objc("alloc")
        pub static alloc(): NSImageView
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSImageView
        @objc("imageViewWithImage:")
        pub static imageViewWithImage(img: NSImage): NSImageView

        @objc("image")
        pub get image(): NSImage
        @objc("setImage:")
        pub set image(img: NSImage)
    }

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

        // NSMenuDelegate protocol — pass an instance of an
        // implementing class through as NSObject.
        @objc("setDelegate:")
        pub setDelegate(d: NSObject)
    }

    /// `NSAlert` — modal dialog with message text, an optional
    /// informative paragraph, an alert style, and one or more
    /// buttons. `runModal` returns the matching
    /// `cocoa.AlertFirstButtonReturn` + N constant for the button
    /// the user clicked. Buttons added via `addButtonWithTitle:`
    /// are ordered right-to-left in the dialog.
    @objc pub class NSAlert : NSObject {
        @objc("alloc")
        pub static alloc(): NSAlert
        @objc("init")
        pub init(): NSAlert

        @objc("setMessageText:")
        pub setMessageText(s: NSString)
        @objc("setInformativeText:")
        pub setInformativeText(s: NSString)
        @objc("setAlertStyle:")
        pub setAlertStyle(style: AlertStyle)
        @objc("addButtonWithTitle:")
        pub addButtonWithTitle(title: NSString): NSButton
        @objc("runModal")
        pub runModal(): i64
    }

    /// `NSSavePanel` — modal "save as" dialog. Configure with
    /// `setNameFieldStringValue:` etc., then `runModal()`. On
    /// `cocoa.ModalResponseOK`, read `URL()` for the chosen path.
    @objc pub class NSSavePanel : NSObject {
        @objc("savePanel")
        pub static savePanel(): NSSavePanel

        @objc("setTitle:")
        pub setTitle(s: NSString)
        @objc("setMessage:")
        pub setMessage(s: NSString)
        @objc("setPrompt:")
        pub setPrompt(s: NSString)
        @objc("setNameFieldStringValue:")
        pub setNameFieldStringValue(s: NSString)
        @objc("setDirectoryURL:")
        pub setDirectoryURL(u: NSURL)
        @objc("runModal")
        pub runModal(): i64
        @objc("URL")
        pub get URL(): NSURL
    }

    /// `NSOpenPanel` — modal "open" dialog. Toggle file / folder
    /// / multi-select via the setters, then `runModal()`. On
    /// `cocoa.ModalResponseOK`, read `URLs()` for the array of
    /// selections, or `URL()` for the single-select case.
    @objc pub class NSOpenPanel : NSSavePanel {
        @objc("openPanel")
        pub static openPanel(): NSOpenPanel

        @objc("setCanChooseFiles:")
        pub setCanChooseFiles(flag: bool)
        @objc("setCanChooseDirectories:")
        pub setCanChooseDirectories(flag: bool)
        @objc("setAllowsMultipleSelection:")
        pub setAllowsMultipleSelection(flag: bool)
        @objc("setResolvesAliases:")
        pub setResolvesAliases(flag: bool)
        @objc("URLs")
        pub get URLs(): NSArray
    }

    /// `NSFont` — typeface + point size. Class methods give you
    /// the system fonts at a chosen size; `fontWithName:size:`
    /// resolves a named family. Pass into `setFont:` on any
    /// text-bearing control.
    @objc pub class NSFont : NSObject {
        @objc("systemFontOfSize:")
        pub static systemFontOfSize(size: f64): NSFont
        @objc("boldSystemFontOfSize:")
        pub static boldSystemFontOfSize(size: f64): NSFont
        @objc("fontWithName:size:")
        pub static fontWithName(name: NSString, size: f64): NSFont

        @objc("familyName")
        pub get familyName(): NSString
        @objc("pointSize")
        pub get pointSize(): f64
    }

    /// `NSScrollView` — wraps a document view (typically an
    /// `NSTextView` or `NSTableView`) and adds scroll bars / a
    /// border. Build via the standard `alloc + initWithFrame:`,
    /// then `setDocumentView:` to hand it the content view.
    @objc pub class NSScrollView : NSView {
        @objc("alloc")
        pub static alloc(): NSScrollView
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSScrollView

        @objc("setDocumentView:")
        pub setDocumentView(v: NSView)
        @objc("setHasVerticalScroller:")
        pub setHasVerticalScroller(flag: bool)
        @objc("setHasHorizontalScroller:")
        pub setHasHorizontalScroller(flag: bool)
        @objc("setAutohidesScrollers:")
        pub setAutohidesScrollers(flag: bool)
        @objc("setBorderType:")
        pub setBorderType(b: BorderType)
        @objc("contentSize")
        pub get contentSize(): NSSize
    }

    /// `NSTextView` — editable multi-line text view. Usually
    /// embedded in an `NSScrollView`. `string` returns the
    /// current buffer as `NSString`; `setString:` replaces it.
    @objc pub class NSTextView : NSView {
        @objc("alloc")
        pub static alloc(): NSTextView
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSTextView

        @objc("string")
        pub get string(): NSString
        @objc("setString:")
        pub set string(s: NSString)
        @objc("setEditable:")
        pub setEditable(flag: bool)
        @objc("setSelectable:")
        pub setSelectable(flag: bool)
        @objc("setFont:")
        pub setFont(f: NSFont)
        @objc("setRichText:")
        pub setRichText(flag: bool)
    }

    /// `NSPopUpButton` — drop-down menu button. `pullsDown=true`
    /// turns it into the menu-button shape (title fixed, items
    /// drop down underneath); `false` is the standard pop-up
    /// (title reflects the selected item).
    @objc pub class NSPopUpButton : NSButton {
        @objc("alloc")
        pub static alloc(): NSPopUpButton
        @objc("initWithFrame:pullsDown:")
        pub init(rect: NSRect, pullsDown: bool): NSPopUpButton

        @objc("addItemWithTitle:")
        pub addItemWithTitle(title: NSString)
        @objc("addItemsWithTitles:")
        pub addItemsWithTitles(titles: NSArray)
        @objc("removeAllItems")
        pub removeAllItems()
        @objc("numberOfItems")
        pub get numberOfItems(): i64
        @objc("indexOfSelectedItem")
        pub get indexOfSelectedItem(): i64
        @objc("titleOfSelectedItem")
        pub get titleOfSelectedItem(): NSString
        @objc("selectItemAtIndex:")
        pub selectItemAtIndex(i: i64)
        @objc("selectItemWithTitle:")
        pub selectItemWithTitle(s: NSString)
    }

    /// `NSComboBox` — editable text field with a drop-down list
    /// of suggestions. Reads its current value via `stringValue`
    /// (inherited from `NSTextField`).
    @objc pub class NSComboBox : NSTextField {
        @objc("alloc")
        pub static alloc(): NSComboBox
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSComboBox

        @objc("addItemWithObjectValue:")
        pub addItemWithObjectValue(o: NSObject)
        @objc("removeAllItems")
        pub removeAllItems()
        @objc("numberOfItems")
        pub get numberOfItems(): i64
        @objc("indexOfSelectedItem")
        pub get indexOfSelectedItem(): i64
        @objc("selectItemAtIndex:")
        pub selectItemAtIndex(i: i64)
        @objc("setNumberOfVisibleItems:")
        pub setNumberOfVisibleItems(n: i64)
    }

    /// `NSProgressIndicator` — bar or spinner. Determinate mode
    /// (default for `.bar`) reads `doubleValue` against the
    /// configured `[min, max]` range; indeterminate mode
    /// requires `startAnimation:` / `stopAnimation:` to spin.
    @objc pub class NSProgressIndicator : NSView {
        @objc("alloc")
        pub static alloc(): NSProgressIndicator
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSProgressIndicator

        @objc("setStyle:")
        pub setStyle(s: ProgressIndicatorStyle)
        @objc("setMinValue:")
        pub setMinValue(v: f64)
        @objc("setMaxValue:")
        pub setMaxValue(v: f64)
        @objc("doubleValue")
        pub get doubleValue(): f64
        @objc("setDoubleValue:")
        pub set doubleValue(v: f64)
        @objc("setIndeterminate:")
        pub setIndeterminate(flag: bool)
        @objc("startAnimation:")
        pub startAnimation(sender: NSObject)
        @objc("stopAnimation:")
        pub stopAnimation(sender: NSObject)
    }

    /// `NSSegmentedControl` — row of buttons with shared
    /// selection. After `setSegmentCount:`, label each slot with
    /// `setLabel:forSegment:`; read the active one via
    /// `selectedSegment`.
    @objc pub class NSSegmentedControl : NSView {
        @objc("alloc")
        pub static alloc(): NSSegmentedControl
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSSegmentedControl

        @objc("segmentCount")
        pub get segmentCount(): i64
        @objc("setSegmentCount:")
        pub set segmentCount(n: i64)
        @objc("setLabel:forSegment:")
        pub setLabel(label: NSString, forSegment: i64)
        @objc("labelForSegment:")
        pub labelForSegment(seg: i64): NSString
        @objc("setSelected:forSegment:")
        pub setSelected(flag: bool, forSegment: i64)
        @objc("isSelectedForSegment:")
        pub isSelectedForSegment(seg: i64): bool
        @objc("selectedSegment")
        pub selectedSegment(): i64
        @objc("setSegmentedTrackingMode:")
        pub setTrackingMode(m: SegmentSwitchTracking)
    }

    /// `NSStepper` — pair of up / down arrows over a value with
    /// configurable bounds and increment. Pair with an
    /// `NSTextField` via target/action when you want a numeric
    /// input.
    @objc pub class NSStepper : NSView {
        @objc("alloc")
        pub static alloc(): NSStepper
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSStepper

        @objc("setMinValue:")
        pub setMinValue(v: f64)
        @objc("setMaxValue:")
        pub setMaxValue(v: f64)
        @objc("setIncrement:")
        pub setIncrement(v: f64)
        @objc("doubleValue")
        pub get doubleValue(): f64
        @objc("setDoubleValue:")
        pub set doubleValue(v: f64)
        @objc("setValueWraps:")
        pub setValueWraps(flag: bool)
    }

    /// `NSDatePicker` — graphical or text-field date editor.
    /// Defaults to `cocoa.DatePickerStyle.textFieldAndStepper`;
    /// switch to `.clockAndCalendar` for the round-clock /
    /// month-calendar combo.
    @objc pub class NSDatePicker : NSView {
        @objc("alloc")
        pub static alloc(): NSDatePicker
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSDatePicker

        @objc("setDatePickerStyle:")
        pub setDatePickerStyle(s: DatePickerStyle)
        @objc("dateValue")
        pub get dateValue(): NSDate
        @objc("setDateValue:")
        pub set dateValue(d: NSDate)
        @objc("setMinDate:")
        pub setMinDate(d: NSDate)
        @objc("setMaxDate:")
        pub setMaxDate(d: NSDate)
    }

    /// `NSBox` — titled / bordered container view. Wraps a
    /// `contentView` you set yourself (default is a blank
    /// `NSView`).
    @objc pub class NSBox : NSView {
        @objc("alloc")
        pub static alloc(): NSBox
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSBox

        @objc("setTitle:")
        pub setTitle(s: NSString)
        @objc("setBorderType:")
        pub setBorderType(b: BorderType)
        @objc("setBoxType:")
        pub setBoxType(t: BoxType)
        @objc("contentView")
        pub get contentView(): NSView
        @objc("setContentView:")
        pub set contentView(v: NSView)
    }

    /// `NSSplitView` — divider-separated stack of subviews the
    /// user can re-proportion. `setVertical:` toggles between a
    /// row of columns (`true`) and a column of rows (`false`).
    @objc pub class NSSplitView : NSView {
        @objc("alloc")
        pub static alloc(): NSSplitView
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSSplitView

        @objc("isVertical")
        pub get isVertical(): bool
        @objc("setVertical:")
        pub set isVertical(flag: bool)
        @objc("setDividerStyle:")
        pub setDividerStyle(style: i64)
        @objc("adjustSubviews")
        pub adjustSubviews()
    }

    /// `NSGridView` — 2-D grid layout. Build top-down with
    /// `addRow:` / `addColumn:` (each takes an `NSArray` of
    /// views); the grid auto-aligns cells. Spacing is per-axis.
    @objc pub class NSGridView : NSView {
        @objc("alloc")
        pub static alloc(): NSGridView
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSGridView
        @objc("gridViewWithViews:")
        pub static gridViewWithViews(rows: NSArray): NSGridView

        @objc("numberOfRows")
        pub get numberOfRows(): i64
        @objc("numberOfColumns")
        pub get numberOfColumns(): i64
        @objc("addRowWithViews:")
        pub addRowWithViews(views: NSArray): NSObject
        @objc("addColumnWithViews:")
        pub addColumnWithViews(views: NSArray): NSObject
        @objc("setRowSpacing:")
        pub setRowSpacing(s: f64)
        @objc("setColumnSpacing:")
        pub setColumnSpacing(s: f64)
    }

    /// `NSPasteboard` — system clipboard. `generalPasteboard`
    /// is the standard "copy / paste" surface; per-type APIs
    /// (`stringForType:` / `setString:forType:`) read or write
    /// individual representations. `clearContents` resets the
    /// pasteboard's change count and removes all data.
    @objc pub class NSPasteboard : NSObject {
        @objc("generalPasteboard")
        pub static generalPasteboard(): NSPasteboard

        @objc("clearContents")
        pub clearContents(): i64
        @objc("setString:forType:")
        pub setString(s: NSString, forType: NSString): bool
        @objc("stringForType:")
        pub stringForType(t: NSString): NSString
    }

    /// `NSWorkspace` — Finder / Launch-Services bridge. Use
    /// `openURL:` to hand a URL off to the default handler app,
    /// or `openFile:` to open a filesystem path in its default
    /// application.
    @objc pub class NSWorkspace : NSObject {
        @objc("sharedWorkspace")
        pub static sharedWorkspace(): NSWorkspace

        @objc("openURL:")
        pub openURL(u: NSURL): bool
        @objc("openFile:")
        pub openFile(path: NSString): bool
    }

    /// `NSCursor` — mouse-cursor sprite. The class methods
    /// return the standard system cursors; `push` adds the
    /// receiver to the cursor stack and `pop` removes the
    /// top of the stack.
    @objc pub class NSCursor : NSObject {
        @objc("arrowCursor")
        pub static arrow(): NSCursor
        @objc("IBeamCursor")
        pub static iBeam(): NSCursor
        @objc("pointingHandCursor")
        pub static pointingHand(): NSCursor
        @objc("crosshairCursor")
        pub static crosshair(): NSCursor
        @objc("openHandCursor")
        pub static openHand(): NSCursor
        @objc("closedHandCursor")
        pub static closedHand(): NSCursor

        @objc("push")
        pub push()
        @objc("pop")
        pub static pop()
        @objc("set")
        pub set()
        @objc("hide")
        pub static hide()
        @objc("unhide")
        pub static unhide()
    }

    /// `NSEvent` — keyboard / mouse / scroll wheel event passed
    /// to `SKScene.mouseDown:` and friends when subclassing for
    /// input. Read the discriminant via `eventType()`, then pull
    /// the type-specific fields (`keyCode`, `characters`,
    /// `locationInWindow`, `deltaX/Y`, `buttonNumber`,
    /// `clickCount`).
    @objc pub class NSEvent : NSObject {
        @objc("type")
        pub get eventType(): EventType
        @objc("locationInWindow")
        pub get locationInWindow(): NSPoint
        @objc("modifierFlags")
        pub get modifierFlags(): EventModifierFlags
        @objc("timestamp")
        pub get timestamp(): f64
        @objc("clickCount")
        pub get clickCount(): i64
        @objc("buttonNumber")
        pub get buttonNumber(): i64
        @objc("deltaX")
        pub get deltaX(): f64
        @objc("deltaY")
        pub get deltaY(): f64
        @objc("deltaZ")
        pub get deltaZ(): f64

        // Keyboard
        @objc("keyCode")
        pub get keyCode(): u16
        @objc("characters")
        pub get characters(): NSString
        @objc("charactersIgnoringModifiers")
        pub get charactersIgnoringModifiers(): NSString
        @objc("isARepeat")
        pub get isARepeat(): bool
    }

    /// `NSScreen` — physical display. `mainScreen` is the one
    /// hosting the menu bar; `screens` enumerates every
    /// connected display. `frame` returns the screen's bounds
    /// in global coordinates; `visibleFrame` subtracts the menu
    /// bar and Dock.
    @objc pub class NSScreen : NSObject {
        @objc("mainScreen")
        pub static mainScreen(): NSScreen
        @objc("screens")
        pub static screens(): NSArray

        @objc("frame")
        pub get frame(): NSRect
        @objc("visibleFrame")
        pub get visibleFrame(): NSRect
    }

    // ─── Delegate / data-source protocols ─────────────────────────
    //
    // `@objc interface` declarations modelled after the Cocoa
    // protocols of the same name. Every method that Apple marks
    // `@optional` is also `@optional` here — the ilang conformance
    // check skips them, and the ObjC runtime decides at dispatch
    // time via `respondsToSelector:`. Argument types follow the
    // Cocoa shape: callbacks named `…DidFoo:` / `…WillFoo:` take
    // a single `NSNotification` whose `object` property is the
    // posting NSApplication / NSWindow / etc.
    //
    // Implementing class side: write a plain top-level `class
    // MyDelegate : NSApplicationDelegate { … }`; the loader's
    // auto-lift converts it into a real Objective-C subclass of
    // NSObject so `app.setDelegate(new MyDelegate())` reaches the
    // method bodies via objc_msgSend.

    /// `NSApplicationDelegate` — callbacks that fire over the
    /// lifecycle of the shared NSApplication. Set as the app's
    /// delegate via `app.setDelegate(myDelegate)` once after
    /// `sharedApplication()`.
    @objc pub interface NSApplicationDelegate {
        @optional applicationWillFinishLaunching(notification: NSNotification)
        @optional applicationDidFinishLaunching(notification: NSNotification)
        @optional applicationWillBecomeActive(notification: NSNotification)
        @optional applicationDidBecomeActive(notification: NSNotification)
        @optional applicationWillResignActive(notification: NSNotification)
        @optional applicationDidResignActive(notification: NSNotification)
        @optional applicationDidHide(notification: NSNotification)
        @optional applicationDidUnhide(notification: NSNotification)
        @optional applicationDidChangeScreenParameters(notification: NSNotification)
        @optional applicationWillTerminate(notification: NSNotification)

        // Decision callbacks. Return-typed selectors that AppKit
        // consults before performing the corresponding action.
        @optional applicationShouldTerminate(app: NSApplication): u64
        @optional applicationShouldTerminateAfterLastWindowClosed(app: NSApplication): bool
        @optional applicationShouldHandleReopen(app: NSApplication, hasVisibleWindows: bool): bool

        // File / URL opens — single-file form is `application:openFile:`,
        // multi-file is `application:openFiles:` (the array variant
        // is the modern preferred one on macOS 10.13+).
        @optional @objc("application:openFile:")
        applicationOpenFile(app: NSApplication, filename: NSString): bool
        @optional @objc("application:openFiles:")
        applicationOpenFiles(app: NSApplication, filenames: NSArray)
        @optional @objc("application:openURLs:")
        applicationOpenURLs(app: NSApplication, urls: NSArray)
    }

    /// `NSWindowDelegate` — per-window lifecycle and geometry
    /// callbacks. Attach with `win.setDelegate(d)`; the same
    /// object can be both the window delegate and the app
    /// delegate by implementing both protocols.
    @objc pub interface NSWindowDelegate {
        @optional windowDidBecomeKey(notification: NSNotification)
        @optional windowDidResignKey(notification: NSNotification)
        @optional windowDidBecomeMain(notification: NSNotification)
        @optional windowDidResignMain(notification: NSNotification)
        @optional windowDidMove(notification: NSNotification)
        @optional windowDidResize(notification: NSNotification)
        @optional windowWillMiniaturize(notification: NSNotification)
        @optional windowDidMiniaturize(notification: NSNotification)
        @optional windowDidDeminiaturize(notification: NSNotification)
        @optional windowDidExpose(notification: NSNotification)
        @optional windowWillClose(notification: NSNotification)
        @optional windowDidChangeScreen(notification: NSNotification)
        @optional windowDidChangeScreenProfile(notification: NSNotification)
        @optional windowDidChangeBackingProperties(notification: NSNotification)

        // Full-screen transition callbacks (NSWindowDelegate's
        // `windowWillEnterFullScreen:` etc.).
        @optional windowWillEnterFullScreen(notification: NSNotification)
        @optional windowDidEnterFullScreen(notification: NSNotification)
        @optional windowWillExitFullScreen(notification: NSNotification)
        @optional windowDidExitFullScreen(notification: NSNotification)

        // Decision callbacks.
        @optional windowShouldClose(sender: NSObject): bool

        // Geometry hooks. `windowWillResize:toSize:` lets the
        // delegate adjust the requested size; the returned size
        // is what the window actually uses.
        @optional @objc("windowWillResize:toSize:")
        windowWillResize(sender: NSWindow, toSize: NSSize): NSSize
        @optional @objc("windowWillUseStandardFrame:defaultFrame:")
        windowWillUseStandardFrame(sender: NSWindow, defaultFrame: NSRect): NSRect
    }

    /// `NSTextFieldDelegate` — text-field-specific edit callbacks.
    /// Inherits the more general
    /// `NSControlTextEditingDelegate` (declared below); use
    /// either protocol depending on whether the methods you care
    /// about are NSTextField-specific or shared with other
    /// NSControl subclasses.
    @objc pub interface NSTextFieldDelegate {
        @optional controlTextDidBeginEditing(notification: NSNotification)
        @optional controlTextDidChange(notification: NSNotification)
        @optional controlTextDidEndEditing(notification: NSNotification)
    }

    /// `NSControlTextEditingDelegate` — generic text-editing
    /// callbacks for any NSControl that wraps a field editor
    /// (NSTextField, NSSearchField, NSComboBox, …).
    /// `control:textShouldBeginEditing:` and friends let the
    /// delegate veto edits, intercept commands, etc.
    @objc pub interface NSControlTextEditingDelegate {
        @optional @objc("control:textShouldBeginEditing:")
        controlTextShouldBeginEditing(control: NSObject, fieldEditor: NSTextView): bool
        @optional @objc("control:textShouldEndEditing:")
        controlTextShouldEndEditing(control: NSObject, fieldEditor: NSTextView): bool
        @optional @objc("control:isValidObject:")
        controlIsValidObject(control: NSObject, obj: NSObject): bool
        @optional @objc("control:didFailToFormatString:errorDescription:")
        controlDidFailToFormatString(
            control: NSObject, string: NSString, errorDescription: NSString
        ): bool
        @optional @objc("control:didFailToValidatePartialString:errorDescription:")
        controlDidFailToValidatePartialString(
            control: NSObject, string: NSString, errorDescription: NSString
        )
    }

    /// `NSMenuDelegate` — menu open / close / item validation
    /// callbacks. Attach via `menu.setDelegate(d)`; commonly used
    /// to populate menus lazily right before they're shown.
    @objc pub interface NSMenuDelegate {
        @optional menuWillOpen(menu: NSMenu)
        @optional menuDidClose(menu: NSMenu)
        @optional menuNeedsUpdate(menu: NSMenu)
        @optional @objc("numberOfItemsInMenu:")
        numberOfItemsInMenu(menu: NSMenu): i64
        @optional @objc("menu:updateItem:atIndex:shouldCancel:")
        updateMenuItem(menu: NSMenu, updateItem: NSMenuItem, atIndex: i64, shouldCancel: bool): bool
        @optional @objc("menu:willHighlightItem:")
        menuWillHighlightItem(menu: NSMenu, willHighlightItem: NSMenuItem)
    }

    // ─── Convenience helpers ─────────────────────────────────────

    /// Fetch the singleton NSApplication, forcing AppKit to load
    /// first. Always call this before any other NSApplication
    /// method so the framework's classes are registered when
    /// `objc_getClass` later runs.
    pub fn sharedApplication(): NSApplication {
        NSApplicationLoad()
        NSApplication.sharedApplication()
    }

    /// Allocate, initialise, and title a window in one go. The
    /// underlying ObjC pattern is
    /// `[[NSWindow alloc] initWithContentRect:... ] setTitle:]`.
    pub fn makeWindow(
        title: string,
        x: f64, y: f64, w: f64, h: f64,
        mask: StyleMask
    ): NSWindow {
        let win = NSWindow.alloc()
            .init(makeRect(x, y, w, h), mask, BackingStore.buffered, false)
        win.setTitle(nsstr(title))
        win
    }

    /// Allocate + frame + title a push button. Defaults to the
    /// rounded `push` bezel — the standard Mac-style push button.
    pub fn makeButton(
        title: string,
        x: f64, y: f64, w: f64, h: f64
    ): NSButton {
        let btn = NSButton.alloc().init(makeRect(x, y, w, h))
        btn.setTitle(nsstr(title))
        btn.setBezelStyle(BezelStyle.push)
        btn
    }

    /// Plain non-editable label backed by an `NSTextField`. Flips
    /// off `editable` / `bordered` / `bezeled` / `drawsBackground`
    /// so it reads as static text rather than an input.
    pub fn makeLabel(
        text: string,
        x: f64, y: f64, w: f64, h: f64
    ): NSTextField {
        let f = NSTextField.alloc().init(makeRect(x, y, w, h))
        f.stringValue = nsstr(text)
        f.setEditable(false)
        f.setBordered(false)
        f.setBezeled(false)
        f.setDrawsBackground(false)
        f.setSelectable(true)
        f
    }

    /// Editable single-line text input pre-seeded with `text`.
    pub fn makeTextField(
        text: string,
        x: f64, y: f64, w: f64, h: f64
    ): NSTextField {
        let f = NSTextField.alloc().init(makeRect(x, y, w, h))
        f.stringValue = nsstr(text)
        f.setEditable(true)
        f.setBezeled(true)
        f
    }

    /// Horizontal slider over `[min, max]` initialised to `value`.
    pub fn makeSlider(
        min: f64, max: f64, value: f64,
        x: f64, y: f64, w: f64, h: f64
    ): NSSlider {
        let s = NSSlider.alloc().init(makeRect(x, y, w, h))
        s.setMinValue(min)
        s.setMaxValue(max)
        s.doubleValue = value
        s
    }

    /// `NSStackView` initialised with the given orientation and
    /// spacing. Position it with `setFrame` and append children
    /// via `addArrangedSubview`.
    pub fn makeStackView(
        orientation: LayoutOrientation,
        spacing: f64
    ): NSStackView {
        let s = NSStackView.alloc().init()
        s.setOrientation(orientation)
        s.setSpacing(spacing)
        s
    }

    /// Load an `NSImage` from a filesystem path. Returns a
    /// wrapper whose `handle` is nil if the file can't be read
    /// (use `img.isValid()` to check before drawing).
    pub fn makeImage(path: string): NSImage {
        NSImage.alloc().initWithContentsOfFile(nsstr(path))
    }

    /// `NSImageView` sized to its image — convenient for laying
    /// out a thumbnail inside a stack view.
    pub fn makeImageView(img: NSImage): NSImageView {
        NSImageView.imageViewWithImage(img)
    }

    /// Simple modal alert. Two-button variant (OK + Cancel by
    /// default — override via the `buttons` array). Returns
    /// `cocoa.AlertFirstButtonReturn`, `…SecondButton…`, etc.
    pub fn runAlert(
        message: string,
        informative: string,
        style: AlertStyle
    ): i64 {
        let a = NSAlert.alloc().init()
        a.setMessageText(nsstr(message))
        a.setInformativeText(nsstr(informative))
        a.setAlertStyle(style)
        a.addButtonWithTitle(nsstr("OK"))
        a.runModal()
    }

    /// Open-file dialog for a single file. Returns the chosen
    /// path as an ilang string, or `none` if the user cancelled.
    pub fn runOpenFileDialog(): string? {
        let p = NSOpenPanel.openPanel()
        p.setCanChooseFiles(true)
        p.setCanChooseDirectories(false)
        p.setAllowsMultipleSelection(false)
        if p.runModal() != ModalResponseOK { return none }
        some(ilstr(p.URL.path))
    }

    /// Save-file dialog with a default filename. Returns the
    /// chosen path, or `none` if the user cancelled.
    pub fn runSaveFileDialog(suggestedName: string): string? {
        let p = NSSavePanel.savePanel()
        p.setNameFieldStringValue(nsstr(suggestedName))
        if p.runModal() != ModalResponseOK { return none }
        some(ilstr(p.URL.path))
    }

    /// Scroll view wrapping `doc` as its document view, with a
    /// vertical scroller and a bezel border. Mirrors the
    /// `NSScrollView` Interface-Builder default.
    pub fn makeScrollView(doc: NSView, x: f64, y: f64, w: f64, h: f64): NSScrollView {
        let s = NSScrollView.alloc().init(makeRect(x, y, w, h))
        s.setHasVerticalScroller(true)
        s.setBorderType(BorderType.bezelBorder)
        s.setDocumentView(doc)
        s
    }

    /// Multi-line editable text view pre-seeded with `text`.
    /// Wrap in `makeScrollView` if you want scroll bars.
    pub fn makeTextView(text: string, x: f64, y: f64, w: f64, h: f64): NSTextView {
        let tv = NSTextView.alloc().init(makeRect(x, y, w, h))
        tv.string = nsstr(text)
        tv.setEditable(true)
        tv
    }

    /// `NSPopUpButton` pre-populated with `items`. The first
    /// entry is selected by default.
    pub fn makePopUp(items: NSArray, x: f64, y: f64, w: f64, h: f64): NSPopUpButton {
        let p = NSPopUpButton.alloc().init(makeRect(x, y, w, h), false)
        p.addItemsWithTitles(items)
        p
    }

    /// Determinate horizontal progress bar over `[min, max]`.
    pub fn makeProgressBar(
        min: f64, max: f64, value: f64,
        x: f64, y: f64, w: f64, h: f64
    ): NSProgressIndicator {
        let p = NSProgressIndicator.alloc().init(makeRect(x, y, w, h))
        p.setStyle(ProgressIndicatorStyle.bar)
        p.setIndeterminate(false)
        p.setMinValue(min)
        p.setMaxValue(max)
        p.doubleValue = value
        p
    }

    /// Copy `s` to the general pasteboard as UTF-8 plain text.
    pub fn pasteboardCopy(s: string) {
        let pb = NSPasteboard.generalPasteboard()
        pb.clearContents()
        pb.setString(nsstr(s), nsstr(PasteboardTypeString))
    }

    /// Read the current UTF-8 plain-text contents of the general
    /// pasteboard, if any.
    pub fn pasteboardPaste(): string? {
        let pb = NSPasteboard.generalPasteboard()
        let v = pb.stringForType(nsstr(PasteboardTypeString))
        if v.handle == 0 { return none }
        some(ilstr(v))
    }

    /// Open `url` in its registered default application via
    /// LaunchServices (`http(s)://`, `mailto:`, `file://`, …).
    pub fn openURL(url: string) {
        let ws = NSWorkspace.sharedWorkspace()
        ws.openURL(NSURL.fromString(nsstr(url)))
    }
}
