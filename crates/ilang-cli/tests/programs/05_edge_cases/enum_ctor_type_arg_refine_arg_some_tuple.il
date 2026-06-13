// Follow-up to enum_ctor_type_arg_refine_in_stores.il (第48弾): the same
// enum-ctor type-arg refinement must reach the remaining positions where
// a constructor like `Result.err("e")` (E fixed, T unfilled) lands —
// argument position (fn / method / init), `some(..)`, and tuple
// elements, including nested composites. `refine_enum_ctor_args` now
// recurses into `some` / tuple / array, and the call-arg checkers invoke
// it, so the unfilled `T` is taken from the declared type instead of
// reaching the monomorphizer as `Any`.
//
// Heap payloads flow through; ARC exact.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// fn argument position.
fn consume(r: Result<Box, string>): i64 {
    match r {
        ok(b) { b.n }
        err(e) { -1 }
    }
}

// method + init argument position.
class Svc {
    held: Result<Box, string>
    init(r: Result<Box, string>) { this.held = r }
    handle(r: Result<Box, string>): i64 {
        match r {
            ok(b) { b.n }
            err(e) { -2 }
        }
    }
}

// fn arg: ok carries a Box. 1 Box/round.
fn argOk(): i64 {
    consume(Result.ok(new Box(7)))
}
// fn arg: err (T unfilled). no Box.
fn argErr(): i64 {
    consume(Result.err("boom"))
}
// some(ctor) against Result<Box,string>?. 1 Box/round.
fn someOk(): i64 {
    let o: Result<Box, string>? = some(Result.ok(new Box(5)))
    match o {
        some(r) {
            match r {
                ok(b) { b.n }
                err(e) { -1 }
            }
        }
        none { -2 }
    }
}
// tuple element + method arg (err) + init arg (ok). 1 Box/round (init).
fn tupleAndMethod(): i64 {
    let t: (Result<Box, string>, i64) = (Result.err("e"), 9)
    let tv = match t[0] {
        ok(b) { b.n }
        err(e) { 0 }
    }
    let s = new Svc(Result.ok(new Box(3)))     // init arg, 1 Box
    tv + s.handle(Result.err("x")) + t[1]      // method arg err (-2)
}

let acc: i64[] = [0]

test.expect(argOk(), 7)
test.expect(argErr(), -1)
test.expect(someOk(), 5)
test.expect(tupleAndMethod(), 0 + (-2) + 9)    // 7

// Per round: argOk 1 + someOk 1 + tupleAndMethod 1 (Svc init Box) = 3.
// (argErr and the err ctors carry no Box.)
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + argOk() + argErr() + someOk() + tupleAndMethod()
    i = i + 1
}
test.expect(deinits[0] - bd, 300)
test.expect(test.liveAllocBytes() - d0, 0)
