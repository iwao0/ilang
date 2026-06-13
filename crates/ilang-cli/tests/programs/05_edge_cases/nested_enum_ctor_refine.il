// `refine_enum_ctor_args` filled an enum ctor's OWN type args from the
// target but never recursed into the ctor's PAYLOAD arguments. So a nested
// ctor like `Result.ok(Maybe.nope)` against `Result<Maybe<Box>, string>`
// left the inner `Maybe.nope` at `T = Any` (and the outer `Result`'s arg
// stayed `Maybe<Any>`). That reached the monomorphizer as `Any` — since
// 第63弾 it surfaces as the "cannot infer the type parameter(s)" diagnostic
// instead, but the construction is actually fully determined by the
// declared type.
//
// Fix: when refining a ctor against a target, (1) replace an own type-arg
// slot that mentions `Any` (bare or nested) with the target's concrete
// arg, and (2) recurse into each payload arg against the substituted
// payload type. Applies at every position that already refines a ctor:
// let annotation, map value / index store, fn return, etc.

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

// let annotation: the inner Maybe.nope adopts Box from the declaration.
let r: Result<Maybe<Box>, string> = Result.ok(Maybe.nope)
let rv = match r {
    ok(mb) {
        match mb {
            some(b) { b.n }
            nope { 0 - 5 }
        }
    }
    err(s) { 0 - 1 }
}
test.expect(rv, 0 - 5)

// double nesting via a fn return.
fn deep(): Result<Maybe<Maybe<Box>>, string> {
    Result.ok(Maybe.some(Maybe.nope))
}
let dv = match deep() {
    ok(mm) {
        match mm {
            some(m) {
                match m {
                    some(b) { b.n }
                    nope { 100 }
                }
            }
            nope { 200 }
        }
    }
    err(s) { 0 - 1 }
}
test.expect(dv, 100)

// map value (index store) of a nested enum, with overwrite + heap payload.
// a = ok(some(Box3)); overwrite with ok(some(Box8)) -> Box3 dies, Box8
// dies when the map drops. 2 Box/round.
fn churn(): i64 {
    let m: Map<string, Result<Maybe<Box>, string>> =
        {"a": Result.ok(Maybe.some(new Box(3)))}
    m["b"] = Result.ok(Maybe.nope)
    m["a"] = Result.ok(Maybe.some(new Box(8)))
    let v = match m.get("a") {
        some(res) {
            match res {
                ok(mb) {
                    match mb {
                        some(b) { b.n }
                        nope { 0 - 5 }
                    }
                }
                err(s) { 0 - 1 }
            }
        }
        none { 0 - 9 }
    }
    v + m.size()                             // 8 + 2 = 10
}

test.expect(churn(), 10)

let acc: i64[] = [0]
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 { acc[0] = acc[0] + churn(); i = i + 1 }
test.expect(deinits[0] - bd, 200)            // 2 Box/round x 100
test.expect(test.liveAllocBytes() - d0, 0)
