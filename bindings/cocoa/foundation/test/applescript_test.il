/// NSAppleScript + NSAppleEventDescriptor — compile + execute a
/// trivial script that returns a fixed integer.

use std.test as test
use foundation { NSAppleScript, NSAppleEventDescriptor,
                 nsstr, ilstr }

// Build + compile.
let s = NSAppleScript.alloc().initWithSource(nsstr("return 42"))
test.expectStr(ilstr(s.source), "return 42")
test.expectTrue(s.compileAndReturnError(0))
test.expectTrue(s.isCompiled)

// Execute returns an NSAppleEventDescriptor wrapping 42.
let result = s.executeAndReturnError(0)
test.expectFalse(result.handle == 0)
test.expect(result.int32Value as i64, 42)

// Descriptor factories: bool / int / string round-trip.
let b = NSAppleEventDescriptor.descriptorWithBoolean(true)
test.expectTrue(b.booleanValue)

let i = NSAppleEventDescriptor.descriptorWithInt32(99)
test.expect(i.int32Value as i64, 99)

let str = NSAppleEventDescriptor.descriptorWithString(nsstr("hi"))
test.expectStr(ilstr(str.stringValue), "hi")

// List descriptor starts empty + we can insert.
let list = NSAppleEventDescriptor.listDescriptor()
test.expect(list.numberOfItems, 0)
list.insertDescriptor(
    NSAppleEventDescriptor.descriptorWithInt32(7), 1
)
test.expect(list.numberOfItems, 1)

0
