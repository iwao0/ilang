// A top-level `let` whose name collides with a class field used to
// hijack the field reference inside that class's methods: the bare
// `Var(name)` resolver in `lower_var_expr` consulted `repl_slots`
// before falling through to the implicit-`this.field` path, so an
// inner method that wrote `field_name` instead read the module slot.
// The symptom was easy to trigger with a leak-detection wrapper —
// `let base = test.liveAllocBytes()` shadowed `Forge.base`, the
// closure-driven `forge.make()` chain read 0 every loop iteration,
// and `test.expect(g().n, i)` failed at i=1.
//
// Fix moved the implicit-this.field branch ahead of the repl_slots
// branch so class members shadow globals the way OOP usually does.
use std.test as test

class Box { n: i64; init(x: i64) { this.n = x } }

class Forge {
    base: i64
    init(b: i64) { this.base = b }
    make(): Box { new Box(base) }
}

class Outer {
    forge: Forge
    init(f: Forge) { this.forge = f }
    builder(): fn(): Box { fn(): Box { forge.make() } }
}

let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    let f = new Forge(i)
    let o = new Outer(f)
    let g = o.builder()
    test.expect(g().n, i)
    i = i + 1
}
let after = test.liveAllocBytes()
test.expectTrue(after - base < 4096)
