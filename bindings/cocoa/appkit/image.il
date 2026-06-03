/// Images + cursors — NSImage / NSImageView / NSCursor.

use foundation { NSObject, NSString, NSURL, NSSize, NSRect, NSPoint, nsstr, makeRect }
use core { NSView }

/// `NSCompositingOperation` — Porter-Duff compositing modes for
/// `NSImage.drawInRect:fromRect:operation:fraction:`. `sourceOver`
/// is the standard "draw on top with alpha"; `copy` overwrites the
/// destination including alpha.
pub enum NSCompositingOperation: u64 {
    clear           = 0
    copy            = 1
    sourceOver      = 2
    sourceIn        = 3
    sourceOut       = 4
    sourceAtop      = 5
    destinationOver = 6
    destinationIn   = 7
    destinationOut  = 8
    destinationAtop = 9
    xor             = 10
    plusDarker      = 11
    plusLighter     = 12
}

@extern(ObjC, "/System/Library/Frameworks/AppKit.framework/AppKit") {
    /// `NSImage` — bitmap / vector image. Construct from a
    /// filesystem path (`fromFile`), an asset-catalog / named
    /// system image (`named` — returns nil if not found), or by
    /// pointing at an `NSURL`. For an empty offscreen bitmap to
    /// draw into, use `alloc().initWithSize(size)` then bracket
    /// the drawing calls with `lockFocus` / `unlockFocus`.
    @objc pub class NSImage : NSObject {
        @objc("alloc")
        pub static alloc(): NSImage
        @objc("initWithContentsOfFile:")
        pub initWithContentsOfFile(path: NSString): NSImage
        @objc("initWithContentsOfURL:")
        pub initWithContentsOfURL(url: NSURL): NSImage
        @objc("initWithSize:")
        pub initWithSize(size: NSSize): NSImage
        @objc("imageNamed:")
        pub static imageNamed(name: NSString): NSImage

        @objc("size")
        pub get size(): NSSize
        @objc("setSize:")
        pub set size(s: NSSize)
        @objc("isValid")
        pub get isValid(): bool

        /// Make this image the current graphics destination — every
        /// `NSBezierPath.fill()` / `stroke()` between this call and
        /// `unlockFocus` rasterises into the image's backing
        /// representation. `lockFocusFlipped:YES` flips the
        /// coordinate system so the origin is top-left (matches the
        /// W3C / `GuiView` convention used by gui-core).
        @objc("lockFocus")
        pub lockFocus()
        @objc("lockFocusFlipped:")
        pub lockFocusFlipped(flipped: bool)
        @objc("unlockFocus")
        pub unlockFocus()

        /// Blit the image into the current graphics context — used
        /// by an `NSView.drawRect:` override to paint a persistent
        /// offscreen bitmap into the view.
        @objc("drawInRect:")
        pub drawInRect(rect: NSRect)
        @objc("drawInRect:fromRect:operation:fraction:")
        pub drawInRectFromRect(
            rect: NSRect, fromRect: NSRect,
            op: NSCompositingOperation, fraction: f64
        )
        @objc("drawAtPoint:fromRect:operation:fraction:")
        pub drawAtPoint(
            point: NSPoint, fromRect: NSRect,
            op: NSCompositingOperation, fraction: f64
        )
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
        /// `NSImageScaling` raw value. `0` = proportionally down,
        /// `1` = axes independent (stretch), `2` = none,
        /// `3` = proportionally up or down.
        @objc("setImageScaling:")
        pub setImageScaling(s: i64)
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
}
