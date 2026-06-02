/// AppKit core — NSApplication / NSView / NSWindow / NSScreen plus
/// the NSWindowStyleMask / NSBackingStoreType /
/// NSApplicationActivationPolicy enums and the app + window
/// delegate protocols. Most other AppKit category files use these
/// classes as bases or arguments.

use foundation { NSObject, NSString, NSArray, NSNotification, NSPoint,
                 NSSize, NSRect, nsstr, makeRect }

// ─── NSWindowStyleMask ──────────────────────────────────────────
// Mirrors Swift's `NSWindow.StyleMask` (raw bit values from
// `<AppKit/NSWindow.h>` `NSWindowStyleMask*`). `borderless` is
// the empty mask; combine the rest with `|`:
//
//     let mask = cocoa.NSWindowStyleMask.titled
//         | cocoa.NSWindowStyleMask.closable
//         | cocoa.NSWindowStyleMask.miniaturizable
//         | cocoa.NSWindowStyleMask.resizable
@flags
pub enum NSWindowStyleMask: u64 {
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

/// NSBackingStoreType — `buffered` is the only non-deprecated
/// member today; AppKit still accepts the legacy values for
/// source-compat.
pub enum NSBackingStoreType: u64 {
    retained    = 0
    nonretained = 1
    buffered    = 2
}

/// NSApplicationActivationPolicy — controls the app's Dock /
/// activation behaviour.
pub enum NSApplicationActivationPolicy: i64 {
    regular    = 0
    accessory  = 1
    prohibited = 2
}

// The `@extern(ObjC, "path")` form names the framework dylib so
// the JIT dlopens it eagerly at startup — that's what registers
// `NSApplication` / `NSWindow` / NSString etc. with libobjc's
// global class registry.
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
        pub setActivationPolicy(policy: NSApplicationActivationPolicy): bool
        @objc("activateIgnoringOtherApps:")
        @deprecated("deprecated in macOS 14.0 — call `activate()` instead.")
        pub activateIgnoringOtherApps(flag: bool)
        @objc("activate")
        pub activate()
        /// Inverse of `activate` — hands foreground / key-window
        /// status to whichever app the system picks next. Used by
        /// `BrowserWindow.blur()`-style callers; no per-window
        /// primitive exists on AppKit.
        @objc("deactivate")
        pub deactivate()
        @objc("run")
        pub run()
        @objc("terminate:")
        pub terminate(sender: NSObject)
        /// Two overloads of `setDelegate:` dispatch the same ObjC
        /// selector: the typed `NSApplicationDelegate` form for the
        /// casual case (compile-time conformance check), and the
        /// permissive `NSObject` form for `setDelegate(nil)` /
        /// framework-provided delegate objects already bound as
        /// bare NSObject.
        @objc("setDelegate:")
        pub setDelegate(d: NSApplicationDelegate)
        @objc("setDelegate:")
        pub setDelegate(d: NSObject)
        @objc("setMainMenu:")
        pub setMainMenu(menu: NSObject)
    }

    /// NSView — base class for every visible AppKit object.
    @objc pub class NSView : NSObject {
        @objc("addSubview:")
        pub addSubview(v: NSView)
        /// `addSubview:positioned:relativeTo:` — explicit z-order.
        /// `place` is `NSWindowOrderingMode` (`-1 = below`,
        /// `+1 = above`). Pass `nil` (cast to `NSView`) for
        /// `relativeTo` to position relative to ALL siblings.
        @objc("addSubview:positioned:relativeTo:")
        pub addSubviewPositioned(v: NSView, place: i64, relativeTo: NSView)
        /// Coordinate conversion. The `fromView` / `toView` slot
        /// is typed `NSObject` so `cocoa.nil` can be passed to
        /// mean "convert from / to window coordinates" — the
        /// same coordinate space `NSEvent.locationInWindow()`
        /// returns. A real NSView wrapper is accepted too
        /// (ObjC dispatches the same way either way).
        @objc("convertPoint:fromView:")
        pub convertPointFromView(p: NSPoint, fromView: NSObject): NSPoint
        @objc("convertPoint:toView:")
        pub convertPointToView(p: NSPoint, toView: NSObject): NSPoint
        @objc("frame")
        pub get frame(): NSRect
        @objc("setFrame:")
        pub setFrame(rect: NSRect)
        @objc("bounds")
        pub get bounds(): NSRect
        @objc("setHidden:")
        pub setHidden(flag: bool)
        @objc("isHidden")
        pub get isHidden(): bool
        @objc("window")
        pub get window(): NSWindow
        /// Mark the receiver dirty so AppKit schedules a
        /// `drawRect:` callback on the next main-loop iteration.
        @objc("setNeedsDisplay:")
        pub setNeedsDisplay(flag: bool)
        /// Layer-hosting setup. Call `setLayer:` *before*
        /// `setWantsLayer:` to take the layer-hosting path (where
        /// the view forwards drawing to your own CALayer subtree
        /// rather than letting AppKit allocate one). Used by GPU
        /// backends that need a CAMetalLayer.
        @objc("setLayer:")
        pub setLayer(layer: NSObject)
        @objc("setWantsLayer:")
        pub setWantsLayer(flag: bool)
    }

    /// NSWindow. The two-step `alloc + initWithContentRect:...`
    /// pattern is the standard Cocoa instance-construction
    /// idiom; the convenience constructor below combines them.
    @objc pub class NSWindow : NSObject {
        @objc("alloc")
        pub static alloc(): NSWindow
        /// `NSWindowStyleMask` / `NSBackingStoreType` are typed
        /// enums on the ilang side. The MIR codegen extracts the
        /// discriminant when handing them to objc_msgSend, so the
        /// C ABI sees the raw NSUInteger that AppKit expects.
        @objc("initWithContentRect:styleMask:backing:defer:")
        pub init(
            rect: NSRect, mask: NSWindowStyleMask, backing: NSBackingStoreType, defer: bool
        ): NSWindow

        @objc("setTitle:")
        pub setTitle(title: NSString)
        @objc("center")
        pub center()
        @objc("makeKeyAndOrderFront:")
        pub makeKeyAndOrderFront(sender: NSObject)
        /// Hide the window without releasing it. Pair with
        /// `makeKeyAndOrderFront:` for show/hide toggling.
        @objc("orderOut:")
        pub orderOut(sender: NSObject)
        /// Resize the content area to `size` (excluding the
        /// titlebar). Preserves the window's top-left position.
        @objc("setContentSize:")
        pub setContentSize(size: NSSize)
        @objc("contentView")
        pub get contentView(): NSView
        @objc("setContentView:")
        pub setContentView(v: NSView)
        /// Hi-DPI backing scale (`1.0` on @1x, `2.0` on retina).
        /// Multiply the view's point dimensions by this to get the
        /// pixel dimensions GPU layers / drawables expect.
        @objc("backingScaleFactor")
        pub get backingScaleFactor(): f64
        /// `acceptsMouseMovedEvents` opt-in for `mouseMoved:` —
        /// AppKit suppresses motion events on windows that didn't
        /// flip it on, so a custom NSView won't see drag-free
        /// movement until this is set.
        @objc("setAcceptsMouseMovedEvents:")
        pub setAcceptsMouseMovedEvents(flag: bool)
        /// Move the responder chain to `r` (typically the
        /// contentView) so keyDown:/keyUp: dispatch lands there.
        /// Returns `true` if the new responder accepted the role.
        @objc("makeFirstResponder:")
        pub makeFirstResponder(r: NSObject): bool
        /// setDelegate overloads — typed `NSWindowDelegate` for
        /// the conformance-checked casual case, NSObject for nil
        /// clearing / framework-bound objects.
        @objc("setDelegate:")
        pub setDelegate(d: NSWindowDelegate)
        @objc("setDelegate:")
        pub setDelegate(d: NSObject)
        /// Window state actions. `miniaturize:` collapses to the
        /// Dock; `deminiaturize:` restores. `zoom:` toggles between
        /// the user frame and the standard ("zoomed") frame — i.e.
        /// the Cocoa equivalent of maximize / unmaximize, but a
        /// single action that flips depending on the current state
        /// (callers check `isZoomed` to disambiguate).
        /// `toggleFullScreen:` is the user-facing full-screen
        /// transition that animates the window onto its own Space
        /// (and back).
        @objc("miniaturize:")
        pub miniaturize(sender: NSObject)
        @objc("deminiaturize:")
        pub deminiaturize(sender: NSObject)
        @objc("zoom:")
        pub zoom(sender: NSObject)
        @objc("toggleFullScreen:")
        pub toggleFullScreen(sender: NSObject)
        @objc("isMiniaturized")
        pub get isMiniaturized(): bool
        @objc("isZoomed")
        pub get isZoomed(): bool
        /// `styleMask` carries `NSWindowStyleMask.fullScreen`
        /// while the window is in full-screen mode; mask against
        /// that bit to query full-screen state.
        @objc("styleMask")
        pub get styleMask(): NSWindowStyleMask
        /// Rewrite the style mask wholesale. Used to toggle
        /// individual bits (`.resizable`, `.closable`,
        /// `.miniaturizable`, …) by reading `styleMask`, OR/AND-
        /// masking, and writing back. AppKit re-frames + re-
        /// chromes the window in response, so callers don't have
        /// to.
        @objc("setStyleMask:")
        pub setStyleMask(mask: NSWindowStyleMask)
        /// Whether the user can drag the window by its title bar
        /// or background. `setMovable(false)` pins the window in
        /// place — the title bar stays drawn but the drag gesture
        /// no longer moves it.
        @objc("isMovable")
        pub get isMovable(): bool
        @objc("setMovable:")
        pub setMovable(flag: bool)
        /// Focus / Z-order. `makeKeyWindow` flips this window into
        /// AppKit's "key window" slot (the keyboard-focus target)
        /// without re-ordering Z; `orderFront:` re-orders Z to the
        /// top without touching key status. Their combination
        /// (`makeKeyAndOrderFront:`) is the all-in-one "show me"
        /// path already bound above. `isKeyWindow` reads the
        /// current key-window state.
        @objc("makeKeyWindow")
        pub makeKeyWindow()
        @objc("orderFront:")
        pub orderFront(sender: NSObject)
        @objc("isKeyWindow")
        pub get isKeyWindow(): bool
        /// `level` is the window-server Z-band the window sits in.
        /// `0` is the normal application band; raising to e.g.
        /// `3` (`NSFloatingWindowLevel`) or `25`
        /// (`NSStatusWindowLevel`) keeps the window above ordinary
        /// peer windows — the "always on top" toggle.
        @objc("level")
        pub get level(): i64
        @objc("setLevel:")
        pub setLevel(level: i64)
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

    // ─── Delegate protocols ─────────────────────────────────────

    /// `NSApplicationDelegate` — callbacks that fire over the
    /// lifecycle of the shared NSApplication. Set as the app's
    /// delegate via `app.setDelegate(myDelegate)` once after
    /// `sharedApplication()`.
    @objc pub interface NSApplicationDelegate {
        applicationWillFinishLaunching?(notification: NSNotification)
        applicationDidFinishLaunching?(notification: NSNotification)
        applicationWillBecomeActive?(notification: NSNotification)
        applicationDidBecomeActive?(notification: NSNotification)
        applicationWillResignActive?(notification: NSNotification)
        applicationDidResignActive?(notification: NSNotification)
        applicationDidHide?(notification: NSNotification)
        applicationDidUnhide?(notification: NSNotification)
        applicationDidChangeScreenParameters?(notification: NSNotification)
        applicationWillTerminate?(notification: NSNotification)

        // Decision callbacks. Return-typed selectors that AppKit
        // consults before performing the corresponding action.
        applicationShouldTerminate?(app: NSApplication): u64
        applicationShouldTerminateAfterLastWindowClosed?(app: NSApplication): bool
        applicationShouldHandleReopen?(app: NSApplication, hasVisibleWindows: bool): bool

        // File / URL opens — single-file form is `application:openFile:`,
        // multi-file is `application:openFiles:` (the array variant
        // is the modern preferred one on macOS 10.13+).
        @objc("application:openFile:")
        applicationOpenFile?(app: NSApplication, filename: NSString): bool
        @objc("application:openFiles:")
        applicationOpenFiles?(app: NSApplication, filenames: NSArray)
        @objc("application:openURLs:")
        applicationOpenURLs?(app: NSApplication, urls: NSArray)
    }

    /// `NSWindowDelegate` — per-window lifecycle and geometry
    /// callbacks. Attach with `win.setDelegate(d)`; the same
    /// object can be both the window delegate and the app
    /// delegate by implementing both protocols.
    @objc pub interface NSWindowDelegate {
        windowDidBecomeKey?(notification: NSNotification)
        windowDidResignKey?(notification: NSNotification)
        windowDidBecomeMain?(notification: NSNotification)
        windowDidResignMain?(notification: NSNotification)
        windowDidMove?(notification: NSNotification)
        windowDidResize?(notification: NSNotification)
        windowWillMiniaturize?(notification: NSNotification)
        windowDidMiniaturize?(notification: NSNotification)
        windowDidDeminiaturize?(notification: NSNotification)
        windowDidExpose?(notification: NSNotification)
        windowWillClose?(notification: NSNotification)
        windowDidChangeScreen?(notification: NSNotification)
        windowDidChangeScreenProfile?(notification: NSNotification)
        windowDidChangeBackingProperties?(notification: NSNotification)

        // Full-screen transition callbacks.
        windowWillEnterFullScreen?(notification: NSNotification)
        windowDidEnterFullScreen?(notification: NSNotification)
        windowWillExitFullScreen?(notification: NSNotification)
        windowDidExitFullScreen?(notification: NSNotification)

        // Decision callbacks.
        windowShouldClose?(sender: NSObject): bool

        // Geometry hooks. `windowWillResize:toSize:` lets the
        // delegate adjust the requested size; the returned size
        // is what the window actually uses.
        @objc("windowWillResize:toSize:")
        windowWillResize?(sender: NSWindow, toSize: NSSize): NSSize
        @objc("windowWillUseStandardFrame:defaultFrame:")
        windowWillUseStandardFrame?(sender: NSWindow, defaultFrame: NSRect): NSRect
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
        mask: NSWindowStyleMask
    ): NSWindow {
        let win = NSWindow.alloc()
            .init(makeRect(x, y, w, h), mask, NSBackingStoreType.buffered, false)
        win.setTitle(nsstr(title))
        win
    }
}
