// The `?` operator desugars to a block whose err arm does
// `return Result.err(e)` — that `Result.err` fixes E (from `e`) but leaves
// T as `Any`, to be refined from the enclosing fn's return type.
// `refine_enum_ctor_args_in_block` walked the body's STATEMENTS for such
// nested returns, and refined the TAIL's own value ctor, but never walked
// the TAIL expression for returns buried inside it. So when a `?` sat
// inside a call argument that is itself the tail —
// `Result.ok(take(g(ok)?))` — the desugared `return Result.err(e)` kept
// T = Any and the monomorphizer aborted with "Type::Any (variadic
// builtins)". A `?` in its own `let` statement worked (statements were
// walked); only the tail-embedded `?` failed.
//
// Fix: also run the full `refine_returns` walk over the tail, so a nested
// `?`'s err return is refined no matter how deep in the tail it sits
// (call arg, some(..), tuple element, array element, double-nested call).
//
// Both the ok path (heap payload unwrapped by `?`, then freed) and the err
// path (early return through the nested desugar) stay ARC-exact.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

fn g(ok: bool): Result<Box, string> {
    if ok { Result.ok(new Box(4)) } else { Result.err("x") }
}

fn take(b: Box): i64 { b.n }
fn wrap(x: i64): i64 { x + 1 }

// `?` nested inside a doubly-nested call argument, as the tail.
fn inDouble(ok: bool): Result<i64, string> {
    Result.ok(wrap(take(g(ok)?)))
}

// `?` nested inside `some(..)`.
fn inSome(ok: bool): Result<i64?, string> {
    Result.ok(some(take(g(ok)?)))
}

// `?` nested inside a tuple element.
fn inTuple(ok: bool): Result<(i64, i64), string> {
    Result.ok((take(g(ok)?), 9))
}

// `?` nested inside an array-literal element.
fn inArray(ok: bool): Result<i64[], string> {
    Result.ok([take(g(ok)?)])
}

// Each ok call makes one Box (unwrapped by `?` and freed); the err call
// makes none (it early-returns through the nested desugar). 4 Box/round.
fn run(): i64 {
    let a = match inDouble(true) {
        ok(v) { v }                          // wrap(take(Box(4))) = 5
        err(s) { 0 }
    }
    let b = match inSome(true) {
        ok(o) {
            match o {
                some(v) { v }                // 4
                none { 0 - 9 }
            }
        }
        err(s) { 0 - 1 }
    }
    let c = match inTuple(true) {
        ok(t) {
            let (x, y) = t
            x + y                            // 4 + 9 = 13
        }
        err(s) { 0 - 1 }
    }
    let d = match inArray(true) {
        ok(xs) { xs[0] }                     // 4
        err(s) { 0 - 1 }
    }
    let e = match inDouble(false) {
        ok(v) { v }
        err(s) { 0 - 5 }                     // err early return: -5
    }
    a + b + c + d + e                        // 5 + 4 + 13 + 4 - 5 = 21
}

test.expect(run(), 21)

let acc: i64[] = [0]
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 { acc[0] = acc[0] + run(); i = i + 1 }
test.expect(deinits[0] - bd, 400)            // 4 Box/round x 100
test.expect(test.liveAllocBytes() - d0, 0)
