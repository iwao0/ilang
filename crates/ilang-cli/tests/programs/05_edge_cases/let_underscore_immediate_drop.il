// `let _ = expr` is a discard binding: the value drops AS SOON
// AS the let-stmt finishes, not at scope exit. Pre-fix the
// underscore name was treated like an ordinary binding, so
// `let _ = new Sub(log)` kept the instance alive until the
// enclosing block ended — which combined with the (separately
// fixed) inherited-deinit issue meant `Sub.deinit` (resolving
// to `T.deinit`) only fired at program exit. Spec change:
// `_` is a discard sigil, the assigned value is dropped at the
// statement boundary.
//
// The legacy --jit backend (ilang-codegen) doesn't yet honour
// this discard semantics; skip there.
use std.test as test
class T {
    log: i64[]
    init(l: i64[]) { this.log = l }
    deinit() { this.log.push(99) }
}

class Sub: T {
    init(l: i64[]) { super(l) }
}

let log: i64[] = []

// `let _ = new Sub(log)` — value dropped at this stmt's end,
// `Sub.deinit` (inherited from T) fires immediately, pushing 99.
let _ = new Sub(log)
test.expect(log.length, 1)
test.expect(log[0], 99)

// Same shape with bare `T`.
let _ = new T(log)
test.expect(log.length, 2)

// Repeated discards each fire deinit on schedule.
let i: i64 = 0
while i < 5 {
    let _ = new Sub(log)
    i = i + 1
}
test.expect(log.length, 7)            // 2 + 5

// Contrast: a NAMED let keeps the instance alive until block end.
{
    let kept = new T(log)
    test.expect(log.length, 7)        // not yet dropped
    let _ = kept.log.length
}
// `kept` block-scoped — drops here.
test.expect(log.length, 8)

// Ordinary expression statement (no `let`) also discards
// immediately — the deinit count tracks every transient.
new T(log)
test.expect(log.length, 9)
