// A generic enum constructor whose type params aren't fully determined
// by its args (e.g. `Result.err("e")` fixes E but not T, `Maybe.nope`
// fixes nothing) must take the unfilled params from the declared type of
// the store it lands in. The `let f: T = ctor` and fn-return paths
// already refined the stashed `enum_ctor_type_args` from the annotation;
// FIELD assignment (explicit + bare), ARRAY-literal elements, and
// MAP-literal values did not. So a 2-param enum like
// `Result<i64, string>` left the unfilled param as `Any`, and the
// monomorphizer aborted with "Type::Any (variadic builtins)" — Result
// (and any 2-param enum) could not be stored in a field or array.
//
// Now every store position refines the ctor's type args. Heap payloads
// flow through and ARC stays exact.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// Result<Box, string> as a class field, set both ways.
class Holder {
    r: Result<Box, string>
    init() { this.r = Result.ok(new Box(7)) }   // explicit this.f = ok(..)
    toErr(): i64 {
        r = Result.err("boom")                  // bare field reassign, T unfilled
        match r {
            ok(b) { b.n }
            err(e) { -1 }
        }
    }
}

// Array literal of Result<Box, string> mixing ok and err.
fn arrayMix(): i64 {
    let rs: Result<Box, string>[] =
        [Result.ok(new Box(1)), Result.err("e"), Result.ok(new Box(3))]
    let sum = 0
    for r in rs {
        sum = sum + match r {
            ok(b) { b.n }
            err(e) { 0 }
        }
    }
    sum                                          // 1 + 0 + 3 = 4
}

// Map literal value of Result<Box, string>.
fn mapValue(): i64 {
    let m: Map<string, Result<Box, string>> =
        {"a": Result.ok(new Box(9)), "b": Result.err("e")}
    match m.get("a") {
        some(r) {
            match r {
                ok(b) { b.n }
                err(e) { -1 }
            }
        }
        none { -2 }
    }
}

// Holder: init makes Box(7) (released by the bare reassign to err). 1/round.
fn field(): i64 {
    let h = new Holder()
    h.toErr()
}

let acc: i64[] = [0]

test.expect(field(), -1)
test.expect(arrayMix(), 4)
test.expect(mapValue(), 9)

// Per round: field 1 (Box7) + arrayMix 2 (Box1, Box3) + mapValue 1 (Box9) = 4.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + field() + arrayMix() + mapValue()
    i = i + 1
}
test.expect(deinits[0] - bd, 400)
test.expect(test.liveAllocBytes() - d0, 0)
