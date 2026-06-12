// A closure stored in a class field that captures only a plain local /
// param used to spuriously capture `this` too: the free-var collector
// added `this` as a candidate for EVERY bare `Var` (in case it was a
// field), and the lowering kept it whenever `this` was resolvable (any
// method body). That made `this.f = fn(){ new Box(x) }` capture the
// receiver, so `this -> closure -> this` was a reference cycle and the
// whole object (plus its closure) leaked.
//
// Fix: `this` is captured ON DEMAND — only when a free var actually
// resolves to a member (field / property) of the enclosing class, i.e.
// a genuine `this.field` desugaring. A closure over a plain local /
// param no longer drags `this` in.
//
// A closure that DOES read a field by bare name still captures `this`
// (and `this.f = fn(){ field }` is then a real, by-design ARC cycle —
// out of scope here; this fixture only pins the spurious case). Exact
// deinit + churn delta=0.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// Closure captures the init PARAM `x` (a plain value), stored in a
// field. No member referenced → no `this` capture → no cycle. Calling
// f() mints a fresh Box that dies after the read. 1 Box/round.
class ParamCap {
    f: fn(): Box
    init(x: i64) { this.f = fn(): Box { new Box(x) } }
}
fn paramCap(): i64 {
    let h = new ParamCap(1)
    h.f().n                                   // 1
}

// Closure captures a heap LOCAL (`k`, bound from a field but captured
// as a value, not via `this`), stored in a field. Still no `this`
// capture. The captured Box dies with the object. 1 Box/round.
class LocalCap {
    keep: Box
    f: fn(): i64
    init(x: i64) {
        this.keep = new Box(x)
        let k = this.keep
        this.f = fn(): i64 { k.n }
    }
}
fn localCap(): i64 {
    let h = new LocalCap(2)
    h.f()                                     // 2
}

// Built-and-dropped without ever calling the closure: the object and
// its (param-capturing) closure must both be freed. No Box made.
fn buildOnly(): i64 {
    let h = new ParamCap(9)
    0
}

// Bare-field closure: reads a field by bare name, so it DOES capture
// `this`. Returned (escapes) rather than stored back into the receiver,
// so no cycle — value must be correct and the Box released. 1 Box/round.
class FieldCap {
    v: Box
    init(x: i64) { this.v = new Box(x) }
    getter(): fn(): i64 { fn(): i64 { v.n } }
}
fn fieldCap(): i64 {
    let h = new FieldCap(3)
    let g = h.getter()
    g()                                       // 3
}

let acc: i64[] = [0]

test.expect(paramCap(), 1)
test.expect(localCap(), 2)
test.expect(buildOnly(), 0)
test.expect(fieldCap(), 3)

// Per round: paramCap 1 + localCap 1 + buildOnly 0 + fieldCap 1 = 3.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + paramCap() + localCap() + buildOnly() + fieldCap()
    i = i + 1
}
test.expect(deinits[0] - bd, 300)
test.expect(test.liveAllocBytes() - d0, 0)
