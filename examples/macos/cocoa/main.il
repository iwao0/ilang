// 🪟 ilang × Cocoa — NSStackView form with an NSApplicationDelegate.
//
// Both ObjC classes are written in the plain top-level form. The
// loader auto-lifts any class whose base list names an
// Objective-C type — either an `@objc interface` (a protocol like
// `NSApplicationDelegate`) or an `@objc class` (a real ObjC class
// like `NSObject`) — into a synthesized `@extern(ObjC) { @objc
// class … }` block with `alloc` / `init` / `register` injected.
//
//   * `AppDelegate : NSApplicationDelegate` implements an ObjC
//     protocol — lift triggered by the interface in the base list.
//   * `FormHandler : NSObject` is a target/action receiver subclass —
//     lift triggered by the ObjC parent class. The button calls
//     `[handler submit:sender]`; the default selector for
//     `submit(sender: …)` is `submit:`, matching what
//     `selectorPtr("submit:")` wants.
//
// Run (the cocoa bindings dep is wired through this directory's
// `ilang.toml`):
//   ilang run examples/macos/cocoa/main.il

use cocoa { NSObject, NSTextField, NSSlider, NSApplicationDelegate,
            NSApplication, NSNotification, NSMenu, NSMenuItem,
            NSWindowStyleMask, NSApplicationActivationPolicy, NSUserInterfaceLayoutOrientation, nil,
            sharedApplication, makeWindow, makeButton, makeLabel,
            makeTextField, makeSlider, makeStackView, makeRect,
            ilstr, nsstr, selectorPtr }

// Module-level state. `textField` / `slider` are captured by the
// FormHandler's `submit:` method; keeping them here lets the
// @objc class close over them without ARC tearing them down
// between events.
let textField = makeTextField("ada", 0.0, 0.0, 0.0, 22.0)
let slider = makeSlider(0.0, 1.0, 0.5, 0.0, 0.0, 0.0, 22.0)

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

class FormHandler : NSObject {
    pub submit(sender: NSObject) {
        console.log("text =", ilstr(textField.stringValue))
        console.log("slider =", slider.doubleValue)
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
//       @objc class FormHandler : NSObject {
//           @objc("alloc") pub static alloc(): FormHandler
//           @objc("init")  pub init(): FormHandler
//
//           @objc("submit:")
//           pub submit(sender: NSObject) {
//               console.log("text =", ilstr(textField.stringValue))
//               console.log("slider =", slider.doubleValue)
//           }
//       }
//   }

let app = sharedApplication()
app.setActivationPolicy(NSApplicationActivationPolicy.regular)

AppDelegate.register()
let appDel = AppDelegate.alloc().init()
app.setDelegate(appDel)

FormHandler.register()
let handler = FormHandler.alloc().init()

let mask = NSWindowStyleMask.titled
    | NSWindowStyleMask.closable
    | NSWindowStyleMask.miniaturizable
    | NSWindowStyleMask.resizable
let win = makeWindow("Cocoa form + delegate", 0.0, 0.0, 360.0, 240.0, mask)
win.center()

// Vertical stack: name label + text field, volume label + slider,
// then the submit button at the bottom.
let stack = makeStackView(NSUserInterfaceLayoutOrientation.vertical, 8.0)
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

// Main menu bar with a File menu that contains a Quit item.
// `terminate:` is NSApplication's standard quit action — leaving
// the target as nil lets the responder chain deliver it to the app.
let mainMenu = NSMenu.alloc().init()
let fileItem = NSMenuItem.alloc().init(nsstr("File"), 0, nsstr(""))
mainMenu.addItem(fileItem)
app.setMainMenu(mainMenu)

let fileMenu = NSMenu.alloc().initWithTitle(nsstr("File"))
fileMenu.addItemWithTitle(nsstr("Quit"), selectorPtr("terminate:"), nsstr("q"))
fileItem.setSubmenu(fileMenu)

win.makeKeyAndOrderFront(nil)
app.activate()
app.run()
