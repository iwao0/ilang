// 🖱️ ilang × Cocoa — minimal click handler.
//
// `ClickHandler : NSObject` is a plain top-level class — the
// loader's auto-lift picks it up because its parent is an
// `@objc class`, generating the NSObject subclass machinery
// (selector wiring, `alloc` / `init` / `register`) automatically.
// `ClickHandler.register()` makes the subclass known to the ObjC
// runtime; AppKit's target/action then dispatches the button
// press via objc_msgSend → our IMP → the ilang `clicked` body.
//
// Run (the cocoa bindings dep is wired through this directory's
// `ilang.toml`):
//   ilang run examples/macos/cocoa_click/main.il
//
// Click the button to see `clicked!` print to stdout. ⌘Q to quit.

use cocoa { NSObject, NSString, NSApplication, NSView, NSWindow, NSButton,
            NSWindowStyleMask, NSApplicationActivationPolicy, nil,
            sharedApplication, makeWindow, makeButton, selectorPtr }

class ClickHandler : NSObject {
    pub clicked(sender: NSObject) {
        console.log("clicked!")
    }
}

// Reference: the equivalent explicit form. Drop down to this
// whenever you need things the auto-lift doesn't expose —
// most importantly raw-pointer parameters / returns (`*u8`,
// `*const char`, etc.) and per-method `@objc("selector:")`
// overrides for selectors that don't follow the default
// `name + ":" per param` shape.
//
//   @extern(ObjC) {
//       @objc class ClickHandler : NSObject {
//           @objc("alloc") static alloc(): ClickHandler
//           @objc("init")  init(): ClickHandler
//
//           @objc("clicked:")
//           clicked(sender: NSObject) {
//               console.log("clicked!")
//           }
//       }
//   }

let app = sharedApplication()
app.setActivationPolicy(NSApplicationActivationPolicy.regular)

let mask = NSWindowStyleMask.titled
    | NSWindowStyleMask.closable
    | NSWindowStyleMask.miniaturizable
    | NSWindowStyleMask.resizable
let win = makeWindow("Cocoa click", 0.0, 0.0, 480.0, 320.0, mask)
win.center()

// Register and instantiate the handler. `register()` is
// idempotent so calling it more than once is safe.
ClickHandler.register()
let handler = ClickHandler.alloc().init()

let btn = makeButton("Click me", 180.0, 140.0, 120.0, 32.0)
btn.setTarget(handler)
btn.setAction(selectorPtr("clicked:"))
win.contentView.addSubview(btn)

win.makeKeyAndOrderFront(nil)
app.activate()
app.run()
