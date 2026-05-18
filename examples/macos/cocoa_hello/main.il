// 👋 ilang × Cocoa — minimal hello window. Opens a 480×320
// window with a "Hello, ilang" button (no click handler — see
// `examples/macos/cocoa_click` for the handler version). Quit
// with ⌘Q.
//
// Run (the cocoa bindings dep is wired through this directory's
// `ilang.toml`):
//   ilang run examples/macos/cocoa_hello/main.il

use cocoa

let app = cocoa.sharedApplication()
app.setActivationPolicy(cocoa.ActivationPolicy.regular)

// Swift's `[.titled, .closable, .miniaturizable, .resizable]`
// in ilang's bitwise-OR form on the typed flags enum.
let mask = cocoa.StyleMask.titled
    | cocoa.StyleMask.closable
    | cocoa.StyleMask.miniaturizable
    | cocoa.StyleMask.resizable
let win = cocoa.makeWindow("ilang × Cocoa", 0.0, 0.0, 480.0, 320.0, mask)
win.center()

let btn = cocoa.makeButton("Hello, ilang", 180.0, 140.0, 120.0, 32.0)
win.contentView.addSubview(btn)

// nil sender — `cocoa.nil` is a pre-built zero-handle NSObject;
// the desugar passes its handle (`0 as *objc_object`, a.k.a. nil)
// through.
win.makeKeyAndOrderFront(cocoa.nil)
app.activate()
app.run()
