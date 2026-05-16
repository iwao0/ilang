// Cocoa app delegate demo.
//
// `AppDelegate : NSObject` implements three NSApplicationDelegate
// callbacks:
//   - `applicationDidFinishLaunching:` runs once at startup.
//   - `applicationShouldTerminateAfterLastWindowClosed:` returns
//     YES, so closing the last window cleanly terminates the app
//     without needing ⌘Q or a separate window delegate.
//   - `applicationWillTerminate:` runs once during shutdown.
//
// The autorelease pool wrapper inside didFinishLaunching is a
// no-op here, but illustrates the shape you want around any loop
// that mints autoreleased Cocoa temporaries (NSStrings, etc.) so
// they drain promptly instead of leaking until the outer thread
// pool drains.
//
// Run with:
//   cargo run --release -p ilang -- run bindings/cocoa/example_delegate.il

use cocoa { NSObject, StyleMask, ACTIVATION_REGULAR,
            sharedApplication, makeWindow, autoreleasepool, nsstr }

@extern(ObjC) {
    @objc class AppDelegate : NSObject {
        @objc("alloc")
        static alloc(): AppDelegate
        @objc("init")
        init(): AppDelegate

        @objc("applicationDidFinishLaunching:")
        didFinishLaunching(notification: NSObject) {
            console.log("AppDelegate: didFinishLaunching")
            autoreleasepool(fn() {
                let i = 0
                while i < 3 {
                    let _s = nsstr("temp")
                    i = i + 1
                }
            })
        }

        // BOOL return — ObjC encodes BOOL as `c` (signed char) on
        // every Apple-supported arch, hence `i8` here.
        @objc("applicationShouldTerminateAfterLastWindowClosed:")
        shouldTerminateAfterLastWindowClosed(app: NSObject): i8 {
            1
        }

        @objc("applicationWillTerminate:")
        willTerminate(notification: NSObject) {
            console.log("AppDelegate: willTerminate")
        }
    }
}

let app = sharedApplication()
app.setActivationPolicy(ACTIVATION_REGULAR)

AppDelegate.register()
let appDel = AppDelegate.alloc().init()
app.setDelegate(appDel)

let mask = StyleMask.titled
    | StyleMask.closable
    | StyleMask.miniaturizable
    | StyleMask.resizable
let win = makeWindow("Cocoa delegate", 0.0, 0.0, 480.0, 320.0, mask)
win.center()

win.makeKeyAndOrderFront(NSObject.wrap(0))
app.activateIgnoringOtherApps(1)
app.run()
