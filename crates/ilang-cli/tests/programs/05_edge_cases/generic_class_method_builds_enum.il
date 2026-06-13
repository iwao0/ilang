// A generic class method that CONSTRUCTS a generic enum used to fail at
// MIR lower with "unknown enum Maybe" / "unknown enum Result". Class
// monomorphization (`class.rs::subst_expr`) substituted type params in a
// specialized method body's expression types, but never re-mangled a
// generic-enum `EnumCtor.enum_name` — so `Maybe.some(this.v)` /
// `Result.ok(this.v)` inside `Wrap<T>` kept its bare `enum_name` after
// `Wrap<i64>` was synthesized. (The `monomorphize_fns` path already did
// this for generic FREE fns; the class path had no equivalent.)
//
// Fix: thread the checker's `enum_ctor_type_args` into the class pass and
// have `subst_expr` re-mangle a generic-enum ctor from its span-recorded
// args, substituting the class's T -> concrete — mirroring the fn path.
// Affects both the built-in `Result` and user-declared generic enums, at
// any nesting depth (`Result.ok(Maybe.some(this.v))`).
//
// Not Result-specific and not fixed here: static members on generic
// classes are rejected by the checker, and a generic fn whose type param
// is fixed only by the return annotation is still unresolved (both noted
// in HANDOFF).

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

enum Maybe<T> {
    some: (T)
    nope
}

class Wrap<T> {
    v: T
    init(x: T) { this.v = x }
    asOk(): Result<T, string> { Result.ok(this.v) }      // built-in Result
    asSome(): Maybe<T> { Maybe.some(this.v) }             // user enum Maybe
    nested(): Result<Maybe<T>, string> {                 // nested ctors
        Result.ok(Maybe.some(this.v))
    }
    // consume a generic enum with explicit `Maybe.` patterns.
    has(m: Maybe<T>): bool {
        match m {
            Maybe.some(x) { true }
            Maybe.nope { false }
        }
    }
}

// Value-level checks: multiple instantiations + nested enum.
let wi = new Wrap<i64>(5)
let ws = new Wrap<string>("hi")
let okI = match wi.asOk() {
    ok(v) { v }
    err(s) { 0 - 1 }
}
test.expect(okI, 5)
let okS = match ws.asOk() {
    ok(s) { s.length }
    err(s) { 0 - 1 }
}
test.expect(okS, 2)
let nest = match wi.nested() {
    ok(m) {
        match m {
            some(v) { v }
            nope { 0 - 9 }
        }
    }
    err(s) { 0 - 2 }
}
test.expect(nest, 5)

// Heap T payload through the generic enums. 2 Box/round:
//   Wrap<Box> holds Box(10); the Maybe.some(Box(99)) arg is freed after
//   `has` matches. asOk / asSome borrow `this.v` (no new Box).
fn run(): i64 {
    let w = new Wrap<Box>(new Box(10))
    let a = match w.asOk() {
        ok(b) { b.n }
        err(s) { 0 - 1 }
    }
    let c = match w.asSome() {
        some(b) { b.n }
        nope { 0 - 2 }
    }
    let f = if w.has(Maybe.some(new Box(99))) { 1 } else { 0 }
    a + c + f                                  // 10 + 10 + 1 = 21
}

test.expect(run(), 21)

let acc: i64[] = [0]
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 { acc[0] = acc[0] + run(); i = i + 1 }
test.expect(deinits[0] - bd, 200)            // 2 Box/round x 100
test.expect(test.liveAllocBytes() - d0, 0)
