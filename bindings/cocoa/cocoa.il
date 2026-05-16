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
    borderless             = 0
    titled                 = 1
    closable               = 2
    miniaturizable         = 4
    resizable              = 8
    utilityWindow          = 16
    docModalWindow         = 64
    nonactivatingPanel     = 128
    texturedBackground     = 256
    unifiedTitleAndToolbar = 4096
    hudWindow              = 8192
    fullScreen             = 16384
    fullSizeContentView    = 32768
}

// NSBackingStoreType — buffered is the only useful value
// today (the others are deprecated).
pub const BACKING_BUFFERED:    u64 = 2

// NSApplicationActivationPolicy — regular makes the app
// appear in the Dock and accept keyboard focus.
pub const ACTIVATION_REGULAR:  i64 = 0

// NSButton.BezelStyle — rounded is the standard macOS push
// button.
pub const BEZEL_ROUNDED:       u64 = 1

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
    }

    /// `NSString` — Cocoa's UTF-16 string type. We only expose
    /// the C-string constructor since ilang code already speaks
    /// UTF-8 natively.
    @objc pub class NSString : NSObject {
        @objc("stringWithUTF8String:")
        pub static fromCStr(s: *const char): NSString
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
        pub setActivationPolicy(policy: i64): i8
        @objc("activateIgnoringOtherApps:")
        pub activateIgnoringOtherApps(flag: i8)
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
        // The underlying ABI is u64 (`NSWindowStyleMask` is a
        // `NSUInteger`). `@flags` enum values are heap-boxed at
        // the ilang level, so they can't ride directly across
        // the objc_msgSend boundary — `makeWindow` below casts
        // `StyleMask as u64` before invoking this.
        @objc("initWithContentRect:styleMask:backing:defer:")
        pub initWithContentRect(
            rect: NSRect, mask: u64, backing: u64, defer: i8
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
        pub initWithFrame(rect: NSRect): NSButton

        @objc("setTitle:")
        pub setTitle(title: NSString)
        @objc("setBezelStyle:")
        pub setBezelStyle(style: u64)

        @objc("setTarget:")
        pub setTarget(target: NSObject)
        // SEL doesn't have a user-facing ilang type today; the
        // action is passed as the raw SEL pointer cast to `i64`.
        // Use `selectorPtr("clicked:")` to build one from a name.
        @objc("setAction:")
        pub setAction(sel: i64)
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
        // `mask as u64` extracts the @flags tag (the OR'd bits),
        // matching the `NSUInteger` shape `objc_msgSend` expects.
        let win = NSWindow.alloc()
            .initWithContentRect(makeRect(x, y, w, h), mask as u64, BACKING_BUFFERED, 0)
        win.setTitle(nsstr(title))
        win
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
    /// rounded bezel — the standard Mac-style push button.
    pub fn makeButton(
        title: string,
        x: f64, y: f64, w: f64, h: f64
    ): NSButton {
        let btn = NSButton.alloc().initWithFrame(makeRect(x, y, w, h))
        btn.setTitle(nsstr(title))
        btn.setBezelStyle(BEZEL_ROUNDED)
        btn
    }
}
