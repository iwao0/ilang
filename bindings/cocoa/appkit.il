// AppKit bindings — the macOS GUI layer (NSApplication / NSWindow
// / NSView / NSButton / NSTextField / NSSlider / NSStackView)
// plus the typed enums and `make*` convenience helpers that go
// with them. Depends on `foundation` for NSObject / NSString /
// geometry types.

use foundation { NSObject, NSString, NSRect, nsstr, makeRect }

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
}
