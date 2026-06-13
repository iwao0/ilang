// Inferring a generic fn's type params from its args recorded the FIRST
// binding per param (`or_insert_with`). An argument like `Result.err("e")`
// pins `E` but leaves `T = Any`; if it came before an argument that does
// fix `T` (`fallback: T` given an `i64`), the `Any` stuck and the fn was
// instantiated as `<Any>` — the monomorphizer then aborted with
// "Type::Any (variadic builtins)". This bit any generic fn taking both an
// under-determined `Result`/enum arg and a `T`-fixing arg, including the
// idiomatic `fn unwrapOr<T>(r: Result<T,string>, fallback: T)` using `?`.
//
// Fix: when collecting type-var bindings, a concrete binding wins over a
// previously-recorded `Any` (two concrete bindings still keep the first,
// so genuine conflicts aren't silently re-inferred).

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// T is fixed by `b`; the `Result` arg only pins E and would leave T = Any.
fn pick<T>(a: Result<T, string>, b: T): T { b }

// `?` inside the generic fn; T is fixed by `fallback`.
fn unwrapOr<T>(r: Result<T, string>, fallback: T): Result<T, string> {
    let v = r?
    Result.ok(v)
}

// Direct value check: arg order puts the Any-giving arg first.
test.expect(pick(Result.err("e"), 42), 42)
test.expect(pick(Result.ok(7), 9), 9)

// Heap T through the `?` helper, ok and err paths. Per round:
//   unwrapOr(ok Box5, Box0) frees Box5 + Box0 (fallback unused) = 2;
//   unwrapOr(err, Box0)     frees Box0 (fallback unused)        = 1.
fn run(): i64 {
    let a = match unwrapOr(Result.ok(new Box(5)), new Box(0)) {
        ok(b) { b.n }
        err(s) { 0 - 1 }
    }
    let c = match unwrapOr(Result.err("boom"), new Box(0)) {
        ok(b) { b.n }
        err(s) { 0 - 2 }
    }
    a + c                                    // 5 - 2 = 3
}

test.expect(run(), 3)

let acc: i64[] = [0]
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 { acc[0] = acc[0] + run(); i = i + 1 }
test.expect(deinits[0] - bd, 300)            // 3 Box/round x 100
test.expect(test.liveAllocBytes() - d0, 0)
