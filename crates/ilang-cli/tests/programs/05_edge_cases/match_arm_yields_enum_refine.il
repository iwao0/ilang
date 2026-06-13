// A `match` whose arms YIELD enum ctors, bound to an unannotated `let`,
// left each arm ctor's type args `Any`. `let res = match r { ok(v) {
// Result.ok(v) } err(e) { Result.err(e) } }` joins to `Result<i64,
// string>`, but `Result.ok(v)` pins only T and `Result.err(e)` only E —
// neither arm completes its own ctor, and with no annotation on `res` (and
// the value flowing out through a separate `res` reference) nothing refined
// them. The unfilled param then reached the monomorphizer as `Any`
// ("Type::Any (variadic builtins)"). Reproduced WITHOUT generics.
//
// Fix: after a match's arms join to a result type, push that type back
// into each arm (`refine_match_arm_ctors`) so a yielded ctor gets its
// sibling-completed type args. Applied in all three match kinds (enum /
// Optional / primitive scrutinee).

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// enum scrutinee: arms yield Result ctors, bound to an unannotated let.
// Non-generic — the bug bit here too. Heap T payload.
fn rewrap(r: Result<Box, string>): Result<Box, string> {
    let res = match r {
        ok(b) { Result.ok(b) }
        err(e) { Result.err(e) }
    }
    res
}

// generic version: T fixed by the scrutinee's payload.
fn rewrapG<T>(r: Result<T, string>): Result<T, string> {
    let res = match r {
        ok(v) { Result.ok(v) }
        err(e) { Result.err(e) }
    }
    res
}

// primitive scrutinee: arms yield Result ctors into an unannotated let.
fn classify(flag: i64): Result<i64, string> {
    let r = match flag {
        0 { Result.ok(10) }
        _ { Result.err("nope") }
    }
    r
}

// value checks (non-heap).
let cg = match rewrapG(Result.ok(99)) {
    ok(v) { v }
    err(s) { 0 - 1 }
}
test.expect(cg, 99)
let cl = match classify(0) {
    ok(v) { v }
    err(s) { 0 - 1 }
}
test.expect(cl, 10)
let ce = match classify(5) {
    ok(v) { v }
    err(s) { 0 - 2 }
}
test.expect(ce, 0 - 2)

// Heap T through the enum-scrutinee rewrap, ok and err. 2 ok Boxes / round.
fn run(): i64 {
    let a = match rewrap(Result.ok(new Box(5))) {
        ok(b) { b.n }
        err(s) { 0 - 1 }
    }
    let b = match rewrap(Result.ok(new Box(6))) {
        ok(b) { b.n }
        err(s) { 0 - 1 }
    }
    let c = match rewrap(Result.err("x")) {
        ok(b) { b.n }
        err(s) { 0 - 2 }
    }
    a + b + c                                // 5 + 6 - 2 = 9
}

test.expect(run(), 9)

let acc: i64[] = [0]
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 { acc[0] = acc[0] + run(); i = i + 1 }
test.expect(deinits[0] - bd, 200)            // 2 Box/round x 100
test.expect(test.liveAllocBytes() - d0, 0)
