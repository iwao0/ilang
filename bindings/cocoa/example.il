// Minimal Cocoa demo — opens a 480×320 window with a "Hello,
// ilang" button. Quit with ⌘Q. No click handler yet (delegate
// support exists via `@objc class MyDelegate : NSObject` but
// isn't exercised here).
//
// Run with:
//   cargo run -p ilang -- run bindings/cocoa/example.il

use cocoa

let app = cocoa.sharedApplication()
app.setActivationPolicy(cocoa.ACTIVATION_REGULAR)

let mask = cocoa.STYLE_TITLED
    | cocoa.STYLE_CLOSABLE
    | cocoa.STYLE_MINIATURIZABLE
    | cocoa.STYLE_RESIZABLE
let win = cocoa.makeWindow("ilang × Cocoa", 0.0, 0.0, 480.0, 320.0, mask)
win.center()

let btn = cocoa.makeButton("Hello, ilang", 180.0, 140.0, 120.0, 32.0)
win.contentView().addSubview(btn)

// nil sender — construct a zero-handle NSObject wrapper; the
// desugar passes `0 as *objc_object` (a.k.a. `nil`) through.
win.makeKeyAndOrderFront(new cocoa.NSObject(0))
app.activateIgnoringOtherApps(1)
app.run()
