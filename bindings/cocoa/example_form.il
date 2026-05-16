// Cocoa demo using NSStackView to lay out an input form.
//
// Builds a window whose content view contains a vertical NSStackView
// with: a label, an editable NSTextField, a label, an NSSlider, and a
// "Submit" button. Clicking the button prints the text field's
// current value and the slider's current double value.
//
// Run with:
//   cargo run --release -p ilang -- run bindings/cocoa/example_form.il

use cocoa { NSObject, NSTextField, NSSlider, NSView,
            StyleMask, ActivationPolicy, LayoutOrientation, nil,
            sharedApplication, makeWindow, makeButton, makeLabel,
            makeTextField, makeSlider, makeStackView, ilstr,
            selectorPtr }

// Module-level state the handler reads. Built below as the
// widgets are assembled; the `submit` callback closes over these
// names when the event loop fires.
let textField = makeTextField("ada", 0.0, 0.0, 0.0, 22.0)
let slider = makeSlider(0.0, 1.0, 0.5, 0.0, 0.0, 0.0, 22.0)

@extern(ObjC) {
    @objc pub class FormHandler : NSObject {
        @objc("alloc")
        pub static alloc(): FormHandler
        @objc("init")
        pub init(): FormHandler

        @objc("submit:")
        pub submit(sender: NSObject) {
            let text = textField.stringValue()
            console.log("text =", ilstr(text))
            console.log("slider =", slider.doubleValue())
        }
    }
}

let app = sharedApplication()
app.setActivationPolicy(ActivationPolicy.regular)

let mask = StyleMask.titled
    | StyleMask.closable
    | StyleMask.miniaturizable
    | StyleMask.resizable
let win = makeWindow("Cocoa form", 0.0, 0.0, 360.0, 240.0, mask)
win.center()

// Build the stack view + children.
let stack = makeStackView(LayoutOrientation.vertical, 8.0)
stack.setFrame(cocoa.makeRect(20.0, 20.0, 320.0, 200.0))

stack.addArrangedSubview(makeLabel("Name:", 0.0, 0.0, 0.0, 20.0))
stack.addArrangedSubview(textField)

stack.addArrangedSubview(makeLabel("Volume:", 0.0, 0.0, 0.0, 20.0))
stack.addArrangedSubview(slider)

FormHandler.register()
let handler = FormHandler.alloc().init()
let submit = makeButton("Submit", 0.0, 0.0, 0.0, 28.0)
submit.setTarget(handler)
submit.setAction(selectorPtr("submit:"))
stack.addArrangedSubview(submit)

win.contentView().addSubview(stack)

win.makeKeyAndOrderFront(nil)
app.activateIgnoringOtherApps(1)
app.run()
