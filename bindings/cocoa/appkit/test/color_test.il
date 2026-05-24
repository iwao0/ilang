/// NSColor — predefined system colours + RGBA factory.

use std.test
use appkit { NSColor, sharedApplication }

let _ = sharedApplication()

// Named system colours are non-nil singletons.
test.expectFalse(NSColor.black.handle == 0)
test.expectFalse(NSColor.white.handle == 0)
test.expectFalse(NSColor.clear.handle == 0)
test.expectFalse(NSColor.red.handle == 0)
test.expectFalse(NSColor.green.handle == 0)
test.expectFalse(NSColor.blue.handle == 0)

// RGBA — components round-trip with bit-exact f64s for the
// device RGB colour space. The component getters only make
// sense for an RGB colour, which our `rgba(...)` factory
// always returns.
let c = NSColor.rgba(0.1, 0.2, 0.3, 0.4)
test.expectF64(c.redComponent, 0.1)
test.expectF64(c.greenComponent, 0.2)
test.expectF64(c.blueComponent, 0.3)
test.expectF64(c.alphaComponent, 0.4)

// Black / white pull alpha=1 and corresponding component values
// when read out via the RGB accessors.
let black = NSColor.rgba(0.0, 0.0, 0.0, 1.0)
test.expectF64(black.alphaComponent, 1.0)

0
