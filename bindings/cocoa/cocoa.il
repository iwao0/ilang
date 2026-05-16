// Minimal Cocoa (AppKit + Foundation) binding for ilang on macOS.
//
// Uses ilang's `@extern(ObjC) { @objc class … }` machinery:
// each `@objc("selector:")` method becomes a typed `objc_msgSend`
// dispatch, return values that name an @objc class get wrapped
// back into ilang instances automatically, and class-args have
// their underlying ObjC `id` extracted before being passed on.
//
// Scope: enough to put an NSWindow on screen with one NSButton.
// No event handling, no delegates — clicking the button does
// nothing, and the only way to close the app is ⌘Q. Delegate
// support exists in ilang (`@objc class MyDelegate : NSObject`)
// but isn't exercised here.

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

// ─── Foundation geometry types (HFA-passed) ──────────────────────
// NSPoint / NSSize / NSRect are plain f64 structs. The AArch64
// ABI rides them through v0..v3 (HFA path) when passed to
// objc_msgSend — same layout C uses, no shim needed.
pub struct NSPoint { x: f64; y: f64 }
pub struct NSSize  { width: f64; height: f64 }
pub struct NSRect  { origin: NSPoint; size: NSSize }

// Forces AppKit (and transitively Foundation + libobjc) to be
// resolved at @lib-probe time so subsequent `objc_getClass` calls
// can find NSApplication / NSWindow / NSButton / NSString.
@extern(C) {
    @lib("/System/Library/Frameworks/AppKit.framework/AppKit")
        pub fn NSApplicationLoad(): i8
}

@extern(ObjC) {
    // ─── Foundation ──────────────────────────────────────────────

    /// Root of the Cocoa class hierarchy. Bound (not subclassed)
    /// so users can call e.g. `obj.release()` on any returned
    /// instance.
    @objc pub class NSObject {
        @objc("release")
        pub release()
        @objc("retain")
        pub retain(): NSObject
        @objc("hash")
        pub hash(): i64

        /// Wrap a raw ObjC `id` (the `i64` pointer value) into an
        /// `NSObject` instance without going through `alloc + init`.
        /// Bridges to the @objc-desugar's hidden `__wrap_handle`
        /// helper, which simply stores `h` into the object's
        /// `handle` field. Use this for `nil` (handle = 0) or for
        /// adopting an `id` returned from a C-side bridge.
        pub static wrap(h: i64): NSObject {
            NSObject.__wrap_handle(h)
        }
    }

    /// `NSString` — Cocoa's UTF-16 string type. We expose the
    /// C-string constructor and `UTF8String` accessor; ilang code
    /// works in UTF-8 so the round-trip is `nsstr(s) ↔ ilstr(ns)`
    /// (see the `ilstr` helper below).
    @objc pub class NSString : NSObject {
        @objc("stringWithUTF8String:")
        pub static fromCStr(s: *const char): NSString
        @objc("UTF8String")
        pub utf8String(): *const char
    }

    // ─── AppKit ──────────────────────────────────────────────────

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

    // libobjc's `sel_registerName` exposed under an ilang-side
    // name with an `i64` return so user code (which can't name
    // raw pointer types) can shuffle a SEL around. The cast back
    // to a typed selector happens automatically inside @objc
    // wrappers; user code just treats SEL as opaque i64.
    @lib("objc") @symbol("sel_registerName")
        fn _sel_register_name(name: *const char): i64

    // ObjC autorelease pool primitives. Mirror `@autoreleasepool {
    // ... }` in Cocoa: push a pool token, run the body, pop the
    // token. Inside the body any `autorelease`-flagged temporaries
    // (e.g. `[NSString stringWithUTF8String:]`) accumulate against
    // this pool instead of the outer thread-default one — pop
    // drains them.
    @lib("objc") @symbol("objc_autoreleasePoolPush")
        fn _autoreleasepool_push(): i64
    @lib("objc") @symbol("objc_autoreleasePoolPop")
        fn _autoreleasepool_pop(token: i64)

    // Heap-allocates an ObjC `void(^)(void)` block whose `invoke`
    // trampoline calls the captured ilang closure. The block is
    // autoreleased before return so callers can pass it straight
    // to APIs that take a completion handler without manual
    // `Block_release`. Implemented in `ilang-runtime/objc_blocks.rs`.
    @lib("c") @symbol("__ilang_make_void_block")
        fn _make_void_block(closure: fn()): i64
    @lib("c") @symbol("__ilang_make_obj_block")
        fn _make_obj_block(closure: fn(i64)): i64
    @lib("c") @symbol("__ilang_make_obj_to_obj_block")
        fn _make_obj_to_obj_block(closure: fn(i64): i64): i64

    /// Intern an Objective-C selector by name and return its
    /// stable SEL pointer (as `i64`). Use it with
    /// `button.setAction(cocoa.selectorPtr("clicked:"))`.
    pub fn selectorPtr(name: string): i64 {
        _sel_register_name(cstrFromString(name))
    }

    /// Build an `NSString` from an ilang `string`. Wraps the
    /// `cstrFromString` + `NSString.fromCStr` two-step.
    pub fn nsstr(s: string): NSString {
        NSString.fromCStr(cstrFromString(s))
    }

    /// Construct an `NSRect` from primitive components.
    pub fn makeRect(x: f64, y: f64, w: f64, h: f64): NSRect {
        let r = new NSRect()
        r.origin.x = x
        r.origin.y = y
        r.size.width = w
        r.size.height = h
        r
    }

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

    /// Build an ObjC `void(^)(void)` block from an ilang closure.
    /// Returns the block as an opaque `i64` pointer suitable for
    /// passing to any `@objc` method whose selector takes a
    /// `void(^)(void)` (completion handlers etc.). The block is
    /// autoreleased — let it die with the surrounding pool, or
    /// retain it via `objc_retain` if you need a longer lifetime.
    pub fn makeBlock(body: fn()): i64 {
        _make_void_block(body)
    }

    /// Build an ObjC `void(^)(id)` block — the callback shape used
    /// by AppKit / Foundation when handing a single object back
    /// (e.g. `addLocalMonitorForEventsMatchingMask:handler:`'s
    /// NSEvent argument). The ilang closure receives the object's
    /// raw `id` as `i64`; wrap it with `NSObject.wrap(arg)` (or a
    /// subclass equivalent) inside if you want a typed view.
    pub fn makeObjectBlock(body: fn(i64)): i64 {
        _make_obj_block(body)
    }

    /// Build an ObjC `id(^)(id)` block — the monitoring callback
    /// shape used by `addLocalMonitorForEventsMatchingMask:handler:`
    /// where the handler returns the event to forward (often the
    /// same one), a replacement, or `0` to swallow. The ilang
    /// closure is `fn(handle: i64): i64` — both sides traffic in
    /// raw `id`s, so wrap them with `NSObject.__wrap_handle_unowned`
    /// for AppKit-owned arguments to avoid double-release.
    pub fn makeObjectToObjectBlock(body: fn(i64): i64): i64 {
        _make_obj_to_obj_block(body)
    }

    /// Run `body` inside a fresh ObjC autorelease pool. Mirrors
    /// Cocoa's `@autoreleasepool { ... }` — any autorelease'd
    /// temporaries (e.g. the NSStrings minted by `nsstr`) get
    /// drained when `body` returns instead of leaking until the
    /// outer thread-default pool drains (which, in a long-running
    /// loop, may never happen).
    pub fn autoreleasepool(body: fn()) {
        let token = _autoreleasepool_push()
        body()
        _autoreleasepool_pop(token)
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

    /// Convert an `NSString` back to an ilang `string` — the
    /// mirror of `nsstr`. Internally `[NSString UTF8String]`
    /// returns a C `const char *` whose lifetime is tied to the
    /// autorelease pool, so call this inside an
    /// `autoreleasepool { ... }` if the string is long-lived or
    /// you'll race the pool drain.
    pub fn ilstr(s: NSString): string {
        stringFromCstr(s.utf8String())
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

/// `nil` sender — an NSObject wrapper around the zero pointer.
/// Pass to AppKit calls that want an arbitrary "no sender"
/// receiver (e.g. `win.makeKeyAndOrderFront(cocoa.nil)`).
pub const nil = NSObject.wrap(0)
