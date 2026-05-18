// 🖱️ ilang × Cocoa — minimal click handler.
//
// Defines `ClickHandler : NSObject` in ilang, registers it with
// the ObjC runtime via `ClickHandler.register()`, and wires the
// button's target/action to `[handler clicked:]`. When the button
// is clicked, AppKit dispatches via objc_msgSend → our IMP → the
// ilang `clicked` body.
//
// Run (the cocoa bindings dep is wired through this directory's
// `ilang.toml`):
//   ilang run examples/macos/cocoa_click/main.il
//
// Click the button to see `clicked!` print to stdout. ⌘Q to quit.

use cocoa { NSObject, NSString, NSApplication, NSView, NSWindow, NSButton,
            StyleMask, ActivationPolicy, nil,
            sharedApplication, makeWindow, makeButton, selectorPtr }

@extern(ObjC) {
    @objc class ClickHandler : NSObject {
        @objc("alloc")
        static alloc(): ClickHandler
        @objc("init")
        init(): ClickHandler

        @objc("clicked:")
        clicked(sender: NSObject) {
            console.log("clicked!")
        }
    }
}

let app = sharedApplication()
app.setActivationPolicy(ActivationPolicy.regular)

let mask = StyleMask.titled
    | StyleMask.closable
    | StyleMask.miniaturizable
    | StyleMask.resizable
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
