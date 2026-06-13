// A generic enum constructor whose type params aren't fully resolved by
// its args (e.g. `Result.err("e")` fixes E but not T) must take the
// unfilled params from the declared type of the store it lands in, or the
// monomorphizer aborts with "Type::Any (variadic builtins)".
//
// Earlier rounds wired this for let / field / array-literal / map-literal /
// fn args / fn-return / closure-return. These store positions were still
// missing the refinement, so a 2-param enum like `Result<Box, string>`
// could not be:
//   - passed as an ENUM PAYLOAD arg, tuple `wrap(Result.err(..))` or
//     struct `wrapS { r: Result.err(..) }`,
//   - pushed/unshifted/filled into / searched in an array via the built-in
//     array methods (`xs.push(Result.err(..))`),
//   - stored via INDEX assignment `xs[0] = Result.err(..)` or
//     `m["k"] = Result.err(..)`.
//
// Now `check_enum_ctor` refines each payload arg from its declared payload
// type, the built-in array methods refine against the element type, and
// `check_assign_index` refines against the element / value type. Heap
// payloads flow through and ARC stays exact.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

enum Wrapper {
    wrap: (Result<Box, string>)
    empty
}

enum WrapperS {
    wrapS: { r: Result<Box, string> }
}

// Enum payload arg, tuple shape: `Wrapper.wrap(Result.ok(..))`. 1 Box/call.
fn payloadTuple(): i64 {
    let w = Wrapper.wrap(Result.ok(new Box(5)))
    match w {
        wrap(r) {
            match r {
                ok(b) { b.n }
                err(e) { 0 - 1 }
            }
        }
        empty { 0 - 2 }
    }
}

// Enum payload arg, struct shape: `WrapperS.wrapS { r: Result.ok(..) }`. 1/call.
fn payloadStruct(): i64 {
    let w = WrapperS.wrapS { r: Result.ok(new Box(6)) }
    match w {
        wrapS { r } {
            match r {
                ok(b) { b.n }
                err(e) { 0 - 1 }
            }
        }
    }
}

// Built-in array method args: push an ok (Box) and an err (no Box). 2/call.
fn arrPush(): i64 {
    let xs: Result<Box, string>[] = [Result.ok(new Box(1))]
    xs.push(Result.err("e"))
    xs.push(Result.ok(new Box(2)))
    let sum = 0
    for r in xs {
        sum = sum + match r {
            ok(b) { b.n }
            err(e) { 0 }
        }
    }
    sum                                      // 1 + 0 + 2 = 3
}

// Array index assignment overwrites the only element; its Box dies on the
// store, the err that replaces it carries none. 1/call.
fn arrIndexAssign(): i64 {
    let xs: Result<Box, string>[] = [Result.ok(new Box(8))]
    xs[0] = Result.err("e")
    match xs[0] {
        ok(b) { b.n }
        err(e) { 0 - 1 }
    }
}

// Map index assignment adds a fresh key (err, no Box); the seeded Box(9)
// dies when the map drops. 1/call.
fn mapIndexAssign(): i64 {
    let m: Map<string, Result<Box, string>> = {"a": Result.ok(new Box(9))}
    m["k"] = Result.err("e")
    match m.get("a") {
        some(r) {
            match r {
                ok(b) { b.n }
                err(e) { 0 - 1 }
            }
        }
        none { 0 - 2 }
    }
}

test.expect(payloadTuple(), 5)
test.expect(payloadStruct(), 6)
test.expect(arrPush(), 3)
test.expect(arrIndexAssign(), -1)
test.expect(mapIndexAssign(), 9)

// Per round: payloadTuple 1 + payloadStruct 1 + arrPush 2 + arrIndexAssign 1
//          + mapIndexAssign 1 = 6.
let acc: i64[] = [0]
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0]
        + payloadTuple() + payloadStruct() + arrPush()
        + arrIndexAssign() + mapIndexAssign()
    i = i + 1
}
test.expect(deinits[0] - bd, 600)
test.expect(test.liveAllocBytes() - d0, 0)
