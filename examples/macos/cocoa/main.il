// 🪟 ilang × Cocoa — NSStackView form with an NSApplicationDelegate.
//
// Combines two patterns:
//   * An `AppDelegate : NSObject` (`@objc class` in `@extern(ObjC)`)
//     that logs the app lifecycle (`applicationDidFinishLaunching:`,
//     `applicationWillTerminate:`) and tells the system to terminate
//     when the last window closes.
//   * A vertical NSStackView form: label + NSTextField, label +
//     NSSlider, and a "Submit" button whose `target/action` calls
//     into a second `@objc class FormHandler` that reads back the
//     field + slider values.
//
// Run (the cocoa bindings dep is wired through this directory's
// `ilang.toml`):
//   ilang run examples/macos/cocoa/main.il

use cocoa { NSObject, NSTextField, NSSlider,
            StyleMask, ActivationPolicy, LayoutOrientation, nil,
            sharedApplication, makeWindow, makeButton, makeLabel,
            makeTextField, makeSlider, makeStackView, makeRect,
            ilstr, selectorPtr }

// Module-level state. `textField` / `slider` are captured by the
// FormHandler's `submit:` method; keeping them here (rather than
// inside `applicationDidFinishLaunching:`) lets the @objc class
// close over them without ARC tearing them down between events.
let textField = makeTextField("ada", 0.0, 0.0, 0.0, 22.0)
let slider = makeSlider(0.0, 1.0, 0.5, 0.0, 0.0, 0.0, 22.0)

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

    @objc class AppDelegate : NSObject {
        @objc("alloc") static alloc(): AppDelegate
        @objc("init")  init(): AppDelegate

        @objc("applicationDidFinishLaunching:")
        didFinishLaunching(notification: NSObject) {
            console.log("AppDelegate: didFinishLaunching")
        }

        // BOOL return; ilang `bool` matches ObjC `BOOL`'s i8 ABI.
        // Returning `true` here ends the app the moment the last
        // window closes, so ⌘Q isn't required.
        @objc("applicationShouldTerminateAfterLastWindowClosed:")
        shouldTerminateAfterLastWindowClosed(app: NSObject): bool {
            true
        }

        @objc("applicationWillTerminate:")
        willTerminate(notification: NSObject) {
            console.log("AppDelegate: willTerminate")
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
