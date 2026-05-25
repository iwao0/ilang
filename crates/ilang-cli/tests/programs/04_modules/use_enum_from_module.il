// Construct + match an enum declared in another module. Both the
// enum value and the helper fn are accessed via the module name.
use std.test as test
use lib_shapes

let c = lib_shapes.Color.green
test.expect(lib_shapes.brightness(c), 2)
test.expect(lib_shapes.brightness(lib_shapes.Color.blue), 3)

// match works on a value-typed `Color` regardless of where the enum
// was declared.
let n = match c {
    red { 10 }
    green { 20 }
    blue { 30 }
}
test.expect(n, 20)
