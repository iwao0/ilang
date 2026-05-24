/// NSButton / NSTextField / NSSlider / NSStepper / NSPopUpButton —
/// construct and round-trip their typed setters / getters
/// without showing them on screen.

use std.test
use appkit { NSButton, NSTextField, NSSlider, NSStepper,
             NSPopUpButton, NSBezelStyle, sharedApplication,
             makeButton, makeLabel, makeTextField, makeSlider }
use foundation { makeRect, NSMutableArray, nsstr, ilstr }

let _ = sharedApplication()

// NSButton via the convenience helper.
let btn = makeButton("Click me", 0.0, 0.0, 100.0, 32.0)
test.expectFalse(btn.handle == 0)
btn.setTitle(nsstr("Renamed"))
btn.setBezelStyle(NSBezelStyle.roundedRect)

// NSTextField — set / get the string value.
let tf = makeTextField("hello", 0.0, 0.0, 200.0, 22.0)
test.expectStr(ilstr(tf.stringValue), "hello")
tf.stringValue = nsstr("world")
test.expectStr(ilstr(tf.stringValue), "world")

// makeLabel sets editable=false; round-trip a value through it
// to confirm the underlying string slot still works.
let lbl = makeLabel("static", 0.0, 0.0, 200.0, 22.0)
test.expectStr(ilstr(lbl.stringValue), "static")

// NSSlider — min / max / value round-trip.
let s = makeSlider(0.0, 100.0, 50.0, 0.0, 0.0, 200.0, 20.0)
test.expectF64(s.doubleValue, 50.0)
s.doubleValue = 75.0
test.expectF64(s.doubleValue, 75.0)

// NSStepper.
let stp = NSStepper.alloc().init(makeRect(0.0, 0.0, 40.0, 24.0))
stp.setMinValue(0.0)
stp.setMaxValue(10.0)
stp.setIncrement(1.0)
stp.doubleValue = 3.0
test.expectF64(stp.doubleValue, 3.0)

// NSPopUpButton.
let pop = NSPopUpButton.alloc().init(makeRect(0.0, 0.0, 150.0, 24.0), false)
test.expect(pop.numberOfItems, 0)
pop.addItemWithTitle(nsstr("alpha"))
pop.addItemWithTitle(nsstr("beta"))
pop.addItemWithTitle(nsstr("gamma"))
test.expect(pop.numberOfItems, 3)
pop.selectItemAtIndex(1)
test.expect(pop.indexOfSelectedItem, 1)
test.expectStr(ilstr(pop.titleOfSelectedItem), "beta")
pop.removeAllItems()
test.expect(pop.numberOfItems, 0)

0
