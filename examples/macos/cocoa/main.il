// 🪟 ilang × Cocoa — NSStackView form with an NSApplicationDelegate.
//
// Two ObjC classes, two ways to write them:
//
//   * `AppDelegate : NSApplicationDelegate` is a plain top-level
//     class that names the `@objc interface` in its base list. The
//     loader's auto-lift turns it into a real ObjC subclass of
//     NSObject behind the scenes (NSObject parent, selector
//     wiring, `alloc` / `init` / `register` injected) so the
//     source stays free of `@extern(ObjC) { @objc class … }`
//     boilerplate. Implement only the lifecycle methods you care
//     about; everything else stays `@optional` no-op.
//
//   * `FormHandler` is a plain target/action receiver — not a
//     protocol implementation. It stays in the explicit
//     `@extern(ObjC) { @objc class FormHandler : NSObject { … } }`
//     form because there's no `@objc interface` for "things that
//     can be a button target"; the binding is selector-based.
//
// Run (the cocoa bindings dep is wired through this directory's
// `ilang.toml`):
//   ilang run examples/macos/cocoa/main.il

use cocoa { NSObject, NSTextField, NSSlider, NSApplicationDelegate,
            NSApplication, NSNotification,
            StyleMask, ActivationPolicy, LayoutOrientation, nil,
            sharedApplication, makeWindow, makeButton, makeLabel,
            makeTextField, makeSlider, makeStackView, makeRect,
            ilstr, selectorPtr }

// Module-level state. `textField` / `slider` are captured by the
// FormHandler's `submit:` method; keeping them here lets the
// @objc class close over them without ARC tearing them down
// between events.
let textField = makeTextField("ada", 0.0, 0.0, 0.0, 22.0)
let slider = makeSlider(0.0, 1.0, 0.5, 0.0, 0.0, 0.0, 22.0)

// New-style ObjC delegate — plain class, auto-lifted by the
// loader because its base list names an `@objc interface`.
// `register()` and `alloc()`/`init()` are synthesized for you.
class AppDelegate : NSApplicationDelegate {
    pub applicationDidFinishLaunching(notification: NSNotification) {
        console.log("AppDelegate: didFinishLaunching")
    }

    // BOOL return; ilang `bool` matches ObjC `BOOL`'s i8 ABI.
    // Returning `true` here ends the app the moment the last
    // window closes, so ⌘Q isn't required.
    pub applicationShouldTerminateAfterLastWindowClosed(app: NSApplication): bool {
        true
    }

    pub applicationWillTerminate(notification: NSNotification) {
        console.log("AppDelegate: willTerminate")
    }
}

// Selector-action receiver — no `@objc interface` to claim, so
// stays in the explicit form.
@extern(ObjC) {
    @objc class FormHandler : NSObject {
        @objc("alloc") pub static alloc(): FormHandler
        @objc("init")  pub init(): FormHandler

        @objc("submit:")
        pub submit(sender: NSObject) {
            console.log("text =", ilstr(textField.stringValue))
            console.log("slider =", slider.doubleValue)
        }
    }
}

let app = sharedApplication()
app.setActivationPolicy(ActivationPolicy.regular)

AppDelegate.register()
let appDel = AppDelegate.alloc().init()
app.setDelegate(appDel)

FormHandler.register()
let handler = FormHandler.alloc().init()

let mask = StyleMask.titled
    | StyleMask.closable
    | StyleMask.miniaturizable
    | StyleMask.resizable
let win = makeWindow("Cocoa form + delegate", 0.0, 0.0, 360.0, 240.0, mask)
win.center()

// Vertical stack: name label + text field, volume label + slider,
// then the submit button at the bottom.
let stack = makeStackView(LayoutOrientation.vertical, 8.0)
stack.setFrame(makeRect(20.0, 20.0, 320.0, 200.0))

stack.addArrangedSubview(makeLabel("Name:", 0.0, 0.0, 0.0, 20.0))
stack.addArrangedSubview(textField)
stack.addArrangedSubview(makeLabel("Volume:", 0.0, 0.0, 0.0, 20.0))
stack.addArrangedSubview(slider)

let submit = makeButton("Submit", 0.0, 0.0, 0.0, 28.0)
submit.setTarget(handler)
submit.setAction(selectorPtr("submit:"))
stack.addArrangedSubview(submit)

win.contentView.addSubview(stack)

win.makeKeyAndOrderFront(nil)
app.activate()
app.run()
