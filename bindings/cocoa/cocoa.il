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
    @objc class NSObject {
        @objc("release") fn release()
        @objc("retain")  fn retain(): NSObject
        @objc("hash")    fn hash(): i64
    }

    /// `NSString` — Cocoa's UTF-16 string type. We only expose
    /// the C-string constructor since ilang code already speaks
    /// UTF-8 natively.
    @objc class NSString : NSObject {
        @objc("stringWithUTF8String:")
            static fn fromCStr(s: *const char): NSString
    }

    // ─── AppKit ──────────────────────────────────────────────────

    /// The shared application object — there's only ever one per
    /// process. Use `cocoa.sharedApplication()` (defined below)
    /// instead of calling this directly so AppKit's framework
    /// load happens first.
    @objc class NSApplication : NSObject {
        @objc("sharedApplication")    static fn sharedApplication(): NSApplication
        @objc("setActivationPolicy:") fn setActivationPolicy(policy: i64): i8
        @objc("activateIgnoringOtherApps:")
                                      fn activateIgnoringOtherApps(flag: i8)
        @objc("run")                  fn run()
    }

    /// NSView — base class for every visible AppKit object.
    @objc class NSView : NSObject {
        @objc("addSubview:") fn addSubview(v: NSView)
    }

    /// NSWindow. The two-step `alloc + initWithContentRect:...`
    /// pattern is the standard Cocoa instance-construction
    /// idiom; the convenience constructor below combines them.
    @objc class NSWindow : NSObject {
        @objc("alloc") static fn alloc(): NSWindow
        @objc("initWithContentRect:styleMask:backing:defer:")
            fn initWithContentRect(
                rect: NSRect, mask: StyleMask, backing: u64, defer: i8
            ): NSWindow

        @objc("setTitle:")           fn setTitle(title: NSString)
        @objc("center")              fn center()
        @objc("makeKeyAndOrderFront:") fn makeKeyAndOrderFront(sender: NSObject)
        @objc("contentView")         fn contentView(): NSView
    }

    /// NSButton. `alloc + initWithFrame:` follows the same
    /// two-step pattern.
    @objc class NSButton : NSView {
        @objc("alloc") static fn alloc(): NSButton
        @objc("initWithFrame:")  fn initWithFrame(rect: NSRect): NSButton

        @objc("setTitle:")       fn setTitle(title: NSString)
        @objc("setBezelStyle:")  fn setBezelStyle(style: u64)
    }

    // ─── Convenience helpers ─────────────────────────────────────

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
            .initWithContentRect(makeRect(x, y, w, h), mask, BACKING_BUFFERED, 0)
        win.setTitle(nsstr(title))
        win
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
