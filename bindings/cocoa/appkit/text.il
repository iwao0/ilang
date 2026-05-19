// Text + typography — NSTextView / NSFont / NSColor.

use foundation { NSObject, NSString, NSRect, nsstr, makeRect }
use app { NSView }

@extern(ObjC, "/System/Library/Frameworks/AppKit.framework/AppKit") {
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

    /// Multi-line editable text view pre-seeded with `text`.
    /// Wrap in `makeScrollView` if you want scroll bars.
    pub fn makeTextView(text: string, x: f64, y: f64, w: f64, h: f64): NSTextView {
        let tv = NSTextView.alloc().init(makeRect(x, y, w, h))
        tv.string = nsstr(text)
        tv.setEditable(true)
        tv
    }
}
