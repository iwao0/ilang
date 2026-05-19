// Images + cursors — NSImage / NSImageView / NSCursor.

use foundation { NSObject, NSString, NSURL, NSSize, NSRect, nsstr, makeRect }
use app { NSView }

@extern(ObjC, "/System/Library/Frameworks/AppKit.framework/AppKit") {
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
