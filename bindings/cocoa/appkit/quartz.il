/// QuartzCore bindings — the minimum surface a GPU-backed view
/// needs: `CAMetalLayer` to host a wgpu / Metal drawable, and
/// `CADisplayLink` to drive frame ticks in sync with the display
/// refresh.
///
/// macOS 14+ for `CADisplayLink` (older releases use
/// CVDisplayLink, which dispatches on a secondary thread and
/// needs main-thread marshalling). This file deliberately targets
/// the newer API to keep the GPU view's frame-tick loop on the
/// main run loop alongside AppKit events.

use foundation { NSObject, NSString, NSSize, NSRect }

@extern(ObjC, "/System/Library/Frameworks/QuartzCore.framework/QuartzCore") {
    /// `CAMetalLayer` — a CALayer subclass whose backing store is
    /// a Metal `MTLTexture` chain. wgpu's
    /// `WGPUSurfaceSourceMetalLayer` holds a raw pointer to one
    /// of these and acquires drawables off it per frame.
    ///
    /// The layer is plain-`alloc`-`init`'d; `device` is set
    /// implicitly by wgpu when the surface is configured, so this
    /// binding doesn't expose `setDevice:`.
    @objc pub class CAMetalLayer : NSObject {
        @objc("alloc")
        pub static alloc(): CAMetalLayer
        @objc("init")
        pub init(): CAMetalLayer

        /// `frame` is in the host view's coordinate space; sync
        /// this on view-resize when the layer is hosted (not
        /// layer-backed).
        @objc("setFrame:")
        pub setFrame(rect: NSRect)
        /// Pixel size of the drawable texture. Independent of
        /// `frame` so callers can opt out of Hi-DPI / under-
        /// sampling. Pass `view.frame.size * contentsScale` for
        /// 1:1 backing pixels.
        @objc("setDrawableSize:")
        pub setDrawableSize(size: NSSize)
        /// Hi-DPI factor (1.0 on @1x, 2.0 on @2x retina). Pulled
        /// from `NSWindow.backingScaleFactor` at view-attach
        /// time.
        @objc("setContentsScale:")
        pub setContentsScale(scale: f64)
        /// Treat the layer as fully opaque — skips the alpha
        /// composite step when the layer covers its host pixel
        /// region.
        @objc("setOpaque:")
        pub setOpaque(flag: bool)
        /// `actions` is a `{ "bounds": NSNull, "position": NSNull, ... }`
        /// dictionary. Setting it to an empty `NSDictionary` makes
        /// the layer skip ALL implicit animations on every geometry
        /// / appearance property — necessary for layer-hosting views
        /// inside live-resize, where CATransaction doesn't suppress
        /// the AppKit-internal animation transaction.
        ///
        /// Typed as `NSObject` (not `NSDictionary`) so callers outside
        /// the `cocoa.foundation` reexport reach can still invoke it.
        @objc("setActions:")
        pub setActions(actions: NSObject)
        /// Layer contents gravity — how the existing layer image
        /// is mapped onto the layer's bounds when bounds change
        /// without a redraw. Default is `kCAGravityResize` (=
        /// "resize") which STRETCHES the last frame to the new
        /// bounds during live window-resize. Pass `nsstr("topLeft")`
        /// to pin to the top-left corner so AppKit clips instead of
        /// stretching, avoiding the visible "slide upward" on grow
        /// and "compress" on shrink.
        @objc("setContentsGravity:")
        pub setContentsGravity(gravity: NSString)
    }


    /// `CADisplayLink` — display-refresh-locked timer. The target
    /// method gets `-(void)tick:(CADisplayLink*)link` on the run
    /// loop the link is added to (typically the main run loop in
    /// `NSRunLoopCommonModes`).
    ///
    /// Available on macOS 14.0+.
    @objc pub class CADisplayLink : NSObject {
        /// `selector` is the SEL pointer from
        /// `selectorPtr("tick:")` — the target method must take a
        /// single `CADisplayLink` argument.
        @objc("displayLinkWithTarget:selector:")
        pub static displayLinkWithTarget(target: NSObject, selector: i64): CADisplayLink

        /// Schedule the link on `runloop` in `mode`. Pair with
        /// `invalidate()` to stop. Use
        /// `nsstr("kCFRunLoopCommonModes")` for `mode` so the link
        /// keeps firing during modal tracking (window drag,
        /// menu tracking, …).
        @objc("addToRunLoop:forMode:")
        pub addToRunLoop(runloop: NSObject, forMode: NSString)
        @objc("removeFromRunLoop:forMode:")
        pub removeFromRunLoop(runloop: NSObject, forMode: NSString)

        /// Pauses firing without unscheduling. The link keeps the
        /// target retained until `invalidate()` is called.
        @objc("setPaused:")
        pub setPaused(flag: bool)
        @objc("isPaused")
        pub get isPaused(): bool

        /// Stop firing and release the target retention. The link
        /// is unusable afterwards.
        @objc("invalidate")
        pub invalidate()
    }

    /// `CATransaction` — groups CALayer geometry / property
    /// mutations into one commit. The class-method-only API is
    /// the easiest way to suppress CALayer's default 0.25 s
    /// implicit animation on geometry changes — wrap the mutation
    /// in `begin()` / `setDisableActions(true)` / `commit()` so
    /// `setFrame:` / `setDrawableSize:` apply instantly. Critical
    /// during live window resize: without this, each
    /// `windowDidResize` re-fires `setFrame:` while the previous
    /// implicit animation is still interpolating, producing
    /// visible sliding / shrinking of layer-backed views.
    @objc pub class CATransaction : NSObject {
        @objc("begin")
        pub static begin()
        @objc("setDisableActions:")
        pub static setDisableActions(flag: bool)
        @objc("commit")
        pub static commit()
    }
}
