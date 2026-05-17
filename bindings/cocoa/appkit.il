// AppKit bindings — the macOS GUI layer (NSApplication / NSWindow
// / NSView / NSButton / NSTextField / NSSlider / NSStackView)
// plus the typed enums and `make*` convenience helpers that go
// with them. Depends on `foundation` for NSObject / NSString /
// geometry types.

use foundation { NSObject, NSString, NSArray, NSURL, NSSize, NSRect, nsstr, ilstr, makeRect }

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

// Force AppKit (and transitively Foundation + libobjc) to be
// resolved at @lib-probe time so subsequent `objc_getClass` calls
// can find NSApplication / NSWindow / NSButton / NSString.
@extern(C) {
    @lib("/System/Library/Frameworks/AppKit.framework/AppKit")
        pub fn NSApplicationLoad(): i8
}

@extern(ObjC) {
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
        pub activateIgnoringOtherApps(flag: bool)
        @objc("run")
        pub run()
        @objc("terminate:")
        pub terminate(sender: NSObject)
        // NSApplicationDelegate is a protocol — any NSObject subclass
        // that implements the relevant `applicationDidFinishLaunching:`
        // / `applicationWillTerminate:` / etc. selectors can be the
        // delegate. ObjC's runtime doesn't enforce protocols, so we
        // just take an NSObject here.
        @objc("setDelegate:")
        pub setDelegate(d: NSObject)
        @objc("setMainMenu:")
        pub setMainMenu(menu: NSMenu)
    }

    /// NSView — base class for every visible AppKit object.
    @objc pub class NSView : NSObject {
        @objc("addSubview:")
        pub addSubview(v: NSView)
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
        pub contentView(): NSView
        // NSWindowDelegate protocol — same ducktype story as
        // NSApplication.setDelegate.
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

        @objc("setStringValue:")
        pub setStringValue(s: NSString)
        @objc("stringValue")
        pub stringValue(): NSString

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
        @objc("setDoubleValue:")
        pub setDoubleValue(v: f64)
        @objc("doubleValue")
        pub doubleValue(): f64

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

    /// `NSColor` — a colour value. Use the named class methods
    /// for system colours (`black`, `red`, `clearColor`, …) or
    /// `colorWithRed:green:blue:alpha:` for RGBA values in
    /// `[0, 1]`. RGBA component accessors only make sense on a
    /// colour that lives in an RGB colour space.
    @objc pub class NSColor : NSObject {
        @objc("blackColor")
        pub static black(): NSColor
        @objc("whiteColor")
        pub static white(): NSColor
        @objc("clearColor")
        pub static clear(): NSColor
        @objc("redColor")
        pub static red(): NSColor
        @objc("greenColor")
        pub static green(): NSColor
        @objc("blueColor")
        pub static blue(): NSColor
        @objc("grayColor")
        pub static gray(): NSColor
        @objc("yellowColor")
        pub static yellow(): NSColor
        @objc("orangeColor")
        pub static orange(): NSColor
        @objc("purpleColor")
        pub static purple(): NSColor

        @objc("colorWithRed:green:blue:alpha:")
        pub static rgba(r: f64, g: f64, b: f64, a: f64): NSColor

        @objc("redComponent")
        pub redComponent(): f64
        @objc("greenComponent")
        pub greenComponent(): f64
        @objc("blueComponent")
        pub blueComponent(): f64
        @objc("alphaComponent")
        pub alphaComponent(): f64
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
        pub size(): NSSize
        @objc("setSize:")
        pub setSize(s: NSSize)
        @objc("isValid")
        pub isValid(): bool
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

        @objc("setImage:")
        pub setImage(img: NSImage)
        @objc("image")
        pub image(): NSImage
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
        pub itemArray(): NSArray
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
        pub URL(): NSURL
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
        pub URLs(): NSArray
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
        f.setStringValue(nsstr(text))
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
        f.setStringValue(nsstr(text))
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
        s.setDoubleValue(value)
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
        some(ilstr(p.URL().path()))
    }

    /// Save-file dialog with a default filename. Returns the
    /// chosen path, or `none` if the user cancelled.
    pub fn runSaveFileDialog(suggestedName: string): string? {
        let p = NSSavePanel.savePanel()
        p.setNameFieldStringValue(nsstr(suggestedName))
        if p.runModal() != ModalResponseOK { return none }
        some(ilstr(p.URL().path()))
    }
}
